//! One tag table every surface shares: favorites, comments, turns and lanes
//! link into `agent_tag` / `agent_tag_link`, and a search reads the tag column.

use std::collections::BTreeMap;

use anyhow::{bail, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::ident::Store;

/// One tag with its use count and last use; `boop tag list` and the recent
/// list read it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    pub tag: String,
    pub created_ts: i64,
    pub last_used_ts: i64,
    pub uses: i64,
}

/// One spelling per tag: trimmed, lowercase, every inner whitespace run one
/// '-'. Nothing left after that is not a tag.
pub fn normalize_tag(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    for word in raw.split_whitespace() {
        if !out.is_empty() {
            out.push('-');
        }
        out.push_str(&word.to_lowercase());
    }
    let out = out.trim_matches('-').to_owned();
    match out.is_empty() {
        true => None,
        false => Some(out),
    }
}

/// The tags a free-text note carries: split on ',' and whitespace, each
/// normalised, first spelling wins on a repeat.
pub fn tags_in(note: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for word in note.split([',', ' ', '\t', '\n', '\r']) {
        let Some(tag) = normalize_tag(word) else {
            continue;
        };
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    tags
}

impl Store {
    /// Upsert the tag, count the use, and link it to `source`. The link is
    /// idempotent on the pair; the use is counted every time.
    pub fn tag_apply(&self, tag: &str, source: &str, now_ms: i64) -> Result<Tag> {
        let Some(tag) = normalize_tag(tag) else {
            bail!("not a tag: {tag:?}");
        };
        let source = source.trim();
        if source.is_empty() {
            bail!("a tag needs a source to hang on");
        }
        self.connection().execute(
            "INSERT INTO agent_tag (tag, created_ts, last_used_ts, uses)
             VALUES (?1, ?2, ?2, 1)
             ON CONFLICT(tag) DO UPDATE
               SET uses = uses + 1, last_used_ts = ?2",
            params![tag, now_ms],
        )?;
        self.connection().execute(
            "INSERT OR IGNORE INTO agent_tag_link (tag, source, ts) VALUES (?1, ?2, ?3)",
            params![tag, source, now_ms],
        )?;
        self.tag(&tag)?
            .ok_or_else(|| anyhow::anyhow!("tag {tag} vanished between write and read"))
    }

    /// Every tag in `note`, applied to `source`. The note itself is not
    /// stored here; the tags are.
    pub fn tags_apply_note(&self, note: &str, source: &str, now_ms: i64) -> Result<Vec<String>> {
        let mut applied = Vec::new();
        for tag in tags_in(note) {
            self.tag_apply(&tag, source, now_ms)?;
            applied.push(tag);
        }
        Ok(applied)
    }

    /// Drop one tag from one source. The tag row and its count stay; other
    /// sources may still carry it. `false` when the pair had no link.
    pub fn tag_unlink(&self, tag: &str, source: &str) -> Result<bool> {
        let Some(tag) = normalize_tag(tag) else {
            return Ok(false);
        };
        let removed = self.connection().execute(
            "DELETE FROM agent_tag_link WHERE tag = ?1 AND source = ?2",
            params![tag, source.trim()],
        )?;
        Ok(removed > 0)
    }

    /// One tag row, or `None` when nothing wears that spelling.
    pub fn tag(&self, tag: &str) -> Result<Option<Tag>> {
        let Some(tag) = normalize_tag(tag) else {
            return Ok(None);
        };
        let row = self
            .connection()
            .query_row(
                "SELECT tag, created_ts, last_used_ts, uses FROM agent_tag WHERE tag = ?1",
                params![tag],
                read_tag,
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                error => Err(error),
            })?;
        Ok(row)
    }

    /// Most recently used first: the recent-five list a prompt offers.
    pub fn tags_recent(&self, limit: usize) -> Result<Vec<Tag>> {
        self.tag_rows(
            "SELECT tag, created_ts, last_used_ts, uses FROM agent_tag
              ORDER BY last_used_ts DESC, tag ASC LIMIT ?1",
            params![limit as i64],
        )
    }

    /// Substring match on the tag column and nothing else, most used first
    /// then most recent. No message body is read here, by design.
    pub fn tags_search(&self, query: &str, limit: usize) -> Result<Vec<Tag>> {
        let needle = query.trim().to_lowercase();
        self.tag_rows(
            "SELECT tag, created_ts, last_used_ts, uses FROM agent_tag
              WHERE instr(tag, ?1) > 0
              ORDER BY uses DESC, last_used_ts DESC, tag ASC LIMIT ?2",
            params![needle, limit as i64],
        )
    }

    /// Every tag, most used first.
    pub fn tags_list(&self) -> Result<Vec<Tag>> {
        self.tag_rows(
            "SELECT tag, created_ts, last_used_ts, uses FROM agent_tag
              ORDER BY uses DESC, last_used_ts DESC, tag ASC",
            params![],
        )
    }

    /// The tags one source carries, oldest link first.
    pub fn tags_for(&self, source: &str) -> Result<Vec<String>> {
        let mut statement = self.connection().prepare(
            "SELECT tag FROM agent_tag_link WHERE source = ?1 ORDER BY ts ASC, tag ASC",
        )?;
        let rows = statement.query_map(params![source.trim()], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<String>>>()?)
    }

    /// The sources one tag hangs on, oldest link first.
    pub fn sources_for(&self, tag: &str) -> Result<Vec<String>> {
        let Some(tag) = normalize_tag(tag) else {
            return Ok(Vec::new());
        };
        let mut statement = self.connection().prepare(
            "SELECT source FROM agent_tag_link WHERE tag = ?1 ORDER BY ts ASC, source ASC",
        )?;
        let rows = statement.query_map(params![tag], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<String>>>()?)
    }

    /// Each non-empty favorite note becomes tags on `favorite:<id>` at the
    /// row's own time. An already-linked favorite is skipped, so a second run
    /// counts no second use; comment notes are prose and stay out.
    pub fn tags_backfill_favorites(&self) -> Result<usize> {
        let mut notes: Vec<(i64, String, i64)> = Vec::new();
        {
            let mut statement = self.connection().prepare(
                "SELECT favorite_id, note, created_ts FROM agent_favorite
                  WHERE note IS NOT NULL AND TRIM(note) <> ''
                  ORDER BY favorite_id",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
            for row in rows {
                notes.push(row?);
            }
        }
        let mut tagged = 0;
        for (favorite_id, note, created_ts) in notes {
            let source = format!("favorite:{favorite_id}");
            if !self.tags_for(&source)?.is_empty() {
                continue;
            }
            if !self.tags_apply_note(&note, &source, created_ts)?.is_empty() {
                tagged += 1;
            }
        }
        Ok(tagged)
    }

    fn tag_rows(&self, sql: &str, values: &[&dyn rusqlite::ToSql]) -> Result<Vec<Tag>> {
        let mut statement = self.connection().prepare(sql)?;
        let rows = statement.query_map(values, read_tag)?;
        Ok(rows.collect::<rusqlite::Result<Vec<Tag>>>()?)
    }
}

/// The source a link wears after a rebuild: `favorite:<old>` follows the
/// favorite to its new id, anything else stands.
pub(crate) fn moved_source(source: &str, moved: &BTreeMap<i64, i64>) -> String {
    let Some(old) = source
        .strip_prefix("favorite:")
        .and_then(|id| id.parse::<i64>().ok())
    else {
        return source.to_owned();
    };
    match moved.get(&old) {
        Some(new) => format!("favorite:{new}"),
        None => source.to_owned(),
    }
}

fn read_tag(row: &rusqlite::Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag {
        tag: row.get(0)?,
        created_ts: row.get(1)?,
        last_used_ts: row.get(2)?,
        uses: row.get(3)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fresh_store(name: &str) -> (PathBuf, Store) {
        let path = std::env::temp_dir().join(format!("boop_tags_{}_{name}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let store = Store::open(path.clone()).unwrap();
        (path, store)
    }

    fn names(tags: &[Tag]) -> Vec<String> {
        tags.iter().map(|tag| tag.tag.clone()).collect()
    }

    #[test]
    fn a_tag_has_one_spelling() {
        assert_eq!(normalize_tag("  Rust Lang "), Some("rust-lang".to_owned()));
        assert_eq!(normalize_tag("   "), None);
        assert_eq!(normalize_tag(""), None);
    }

    #[test]
    fn a_note_splits_into_tags_on_commas_and_whitespace() {
        assert_eq!(tags_in("rust, perf review"), ["rust", "perf", "review"]);
        assert_eq!(tags_in("rust rust,RUST"), ["rust"]);
        assert!(tags_in("   ").is_empty());
    }

    /// The link is idempotent on the pair; the use still counts, so a tag a
    /// caller keeps reaching for keeps rising in the list.
    #[test]
    fn applying_the_same_tag_twice_counts_two_uses_and_keeps_one_link() {
        let (path, store) = fresh_store("apply-twice");
        store.tag_apply("rust", "favorite:1", 10).unwrap();
        let tag = store.tag_apply("rust", "favorite:1", 20).unwrap();
        assert_eq!(tag.uses, 2);
        assert_eq!(tag.created_ts, 10);
        assert_eq!(tag.last_used_ts, 20);
        assert_eq!(store.sources_for("rust").unwrap(), ["favorite:1"]);
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn recent_puts_the_last_used_tag_first() {
        let (path, store) = fresh_store("recent-order");
        store.tag_apply("a", "s1", 10).unwrap();
        store.tag_apply("b", "s1", 20).unwrap();
        store.tag_apply("c", "s1", 30).unwrap();
        store.tag_apply("a", "s2", 40).unwrap();
        assert_eq!(names(&store.tags_recent(10).unwrap()), ["a", "c", "b"]);
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// The ask: five recent tags, not the whole table.
    #[test]
    fn recent_stops_at_the_limit() {
        let (path, store) = fresh_store("recent-limit");
        for (index, tag) in ["a", "b", "c", "d", "e", "f", "g"].iter().enumerate() {
            store.tag_apply(tag, "s1", 10 + index as i64).unwrap();
        }
        assert_eq!(
            names(&store.tags_recent(5).unwrap()),
            ["g", "f", "e", "d", "c"]
        );
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn search_matches_the_tag_column_most_used_first() {
        let (path, store) = fresh_store("search-order");
        store.tag_apply("rust", "s1", 10).unwrap();
        store.tag_apply("rust", "s2", 11).unwrap();
        store.tag_apply("trust", "s3", 12).unwrap();
        store.tag_apply("docs", "s4", 13).unwrap();
        assert_eq!(names(&store.tags_search("rus", 20).unwrap()), ["rust", "trust"]);
        assert_eq!(names(&store.tags_search("RUS", 20).unwrap()), ["rust", "trust"]);
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// The ask: a tag search never walks message bodies, so prose sitting in
    /// a favorite note is unfindable through it.
    #[test]
    fn search_never_reads_a_note_body() {
        let (path, store) = fresh_store("search-no-prose");
        store
            .favorite_add("# body\n", Some("rust in prose"), "chat", 1)
            .unwrap();
        assert!(store.tags_search("prose", 20).unwrap().is_empty());
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_tag_reads_both_directions() {
        let (path, store) = fresh_store("both-directions");
        store.tag_apply("x", "s1", 10).unwrap();
        store.tag_apply("x", "s2", 11).unwrap();
        assert_eq!(store.tags_for("s1").unwrap(), ["x"]);
        assert_eq!(store.sources_for("x").unwrap(), ["s1", "s2"]);
        assert!(store.tag_unlink("x", "s1").unwrap());
        assert!(store.tags_for("s1").unwrap().is_empty());
        assert!(!store.tag_unlink("x", "s1").unwrap());
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn backfill_turns_favorite_notes_into_tags_once() {
        let (path, store) = fresh_store("backfill");
        let first = store
            .favorite_add("# one\n", Some("rust, perf"), "chat", 5)
            .unwrap();
        let second = store
            .favorite_add("# two\n", Some("docs"), "chat", 6)
            .unwrap();
        store.favorite_add("# three\n", None, "chat", 7).unwrap();
        assert_eq!(store.tags_backfill_favorites().unwrap(), 2);
        assert_eq!(
            store.tags_for(&format!("favorite:{first}")).unwrap(),
            ["perf", "rust"],
            "one note, one timestamp, so the tie breaks on the tag"
        );
        assert_eq!(
            store.tags_for(&format!("favorite:{second}")).unwrap(),
            ["docs"]
        );
        assert_eq!(store.tag("rust").unwrap().unwrap().uses, 1);
        assert_eq!(store.tags_backfill_favorites().unwrap(), 0);
        assert_eq!(store.tag("rust").unwrap().unwrap().uses, 1);
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// Tags are user-authored like favorites: a rebuild drops every projected
    /// row and the counts, the timestamps and both link kinds come back.
    #[test]
    fn tags_and_their_links_survive_a_rebuild() {
        let (path, store) = fresh_store("rebuild");
        let dropped = store.favorite_add("# gone\n", None, "chat", 4).unwrap();
        let favorite = store
            .favorite_add("# pinned\n", Some("kept"), "chat", 5)
            .unwrap();
        store.favorite_delete(dropped).unwrap();
        store
            .tag_apply("rust", &format!("favorite:{favorite}"), 10)
            .unwrap();
        store
            .tag_apply("perf", &format!("favorite:{favorite}"), 11)
            .unwrap();
        store.tag_apply("rust", "lane:x", 12).unwrap();
        let before = store.tags_list().unwrap();

        store.rebuild().unwrap();

        assert_eq!(store.tags_list().unwrap(), before);
        let moved: i64 = store
            .connection()
            .query_row("SELECT favorite_id FROM agent_favorite", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(moved, favorite, "a favorite comes back wearing its own id");
        assert_eq!(
            store.tags_for(&format!("favorite:{moved}")).unwrap(),
            ["rust", "perf"]
        );
        assert_eq!(store.tags_for("lane:x").unwrap(), ["rust"]);
        assert_eq!(
            store.sources_for("rust").unwrap(),
            [format!("favorite:{moved}"), "lane:x".to_owned()]
        );
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// A comment, its two targets and its fork are user-authored too: the
    /// rebuild carries every column, the comment id included, so the tag on
    /// `comment:<id>` still names it afterwards.
    #[test]
    fn a_comment_its_targets_and_its_fork_survive_a_rebuild() {
        let (path, store) = fresh_store("rebuild-comment");
        let earlier = [("ses-z".to_owned(), 1)];
        store
            .turn_comment_upsert(&crate::ident::TurnCommentUpsert {
                client_id: "item-0",
                kind: "selection",
                quote: "gone",
                note: None,
                enabled: true,
                tab_name: None,
                targets: &earlier,
                ts: 99,
            })
            .unwrap();
        store.turn_comment_delete("item-0").unwrap();
        let targets = [("ses-a".to_owned(), 7), ("ses-b".to_owned(), 9)];
        let comment_id = store
            .turn_comment_upsert(&crate::ident::TurnCommentUpsert {
                client_id: "item-1",
                kind: "selection",
                quote: "the line as read",
                note: Some("fix this"),
                enabled: true,
                tab_name: Some("pane-3"),
                targets: &targets,
                ts: 100,
            })
            .unwrap();
        store
            .record_turn_comment_fork(&crate::ident::TurnCommentFork {
                comment_id,
                lane: "feature-x".to_owned(),
                branch: "feature/x".to_owned(),
                brief: "/tmp/brief.md".to_owned(),
                created_ts: 101,
            })
            .unwrap();
        store
            .tag_apply("review", &format!("comment:{comment_id}"), 102)
            .unwrap();
        let before = store.turn_comment(comment_id).unwrap().unwrap();

        store.rebuild().unwrap();

        let after = store.turn_comment(comment_id).unwrap().unwrap();
        assert_eq!(after.comment_id, before.comment_id);
        assert_eq!(after.client_id, "item-1");
        assert_eq!(after.quote, "the line as read");
        assert_eq!(after.note.as_deref(), Some("fix this"));
        assert!(after.enabled);
        assert_eq!(after.tab_name.as_deref(), Some("pane-3"));
        assert_eq!((after.created_ts, after.updated_ts), (100, 100));
        assert_eq!(
            after
                .targets
                .iter()
                .map(|target| (target.session.clone(), target.turn))
                .collect::<Vec<_>>(),
            [("ses-a".to_owned(), 7), ("ses-b".to_owned(), 9)],
            "a target keeps its session by name, not by dict id"
        );
        assert_eq!(
            store.turn_comment_forks(comment_id).unwrap(),
            [crate::ident::TurnCommentFork {
                comment_id,
                lane: "feature-x".to_owned(),
                branch: "feature/x".to_owned(),
                brief: "/tmp/brief.md".to_owned(),
                created_ts: 101,
            }]
        );
        assert_eq!(
            store.tags_for(&format!("comment:{comment_id}")).unwrap(),
            ["review"]
        );
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// A favorite is markdown plus a note; the body re-interns and every other
    /// column, its id included, reads back as it was.
    #[test]
    fn a_favorite_survives_a_rebuild_whole() {
        let (path, store) = fresh_store("rebuild-favorite");
        let id = store
            .favorite_add("# pinned\n", Some("why"), "ses-a:12", 42)
            .unwrap();
        let before = store.query_favorite(id).unwrap();

        store.rebuild().unwrap();

        assert_eq!(store.query_favorite(id).unwrap(), before);
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// The guard behind the tag-link restore: if a favorite ever came back
    /// with a new id, its links would follow it.
    #[test]
    fn a_link_source_follows_a_moved_favorite() {
        let moved = BTreeMap::from([(2, 1)]);
        assert_eq!(moved_source("favorite:2", &moved), "favorite:1");
        assert_eq!(moved_source("favorite:9", &moved), "favorite:9");
        assert_eq!(moved_source("lane:x", &moved), "lane:x");
        assert_eq!(moved_source("comment:2", &moved), "comment:2");
    }

    #[test]
    fn a_note_applies_every_tag_it_holds() {
        let (path, store) = fresh_store("apply-note");
        let applied = store
            .tags_apply_note("rust, perf review", "comment:3", 9)
            .unwrap();
        assert_eq!(applied, ["rust", "perf", "review"]);
        assert_eq!(names(&store.tags_list().unwrap()).len(), 3);
        drop(store);
        let _ = std::fs::remove_file(&path);
    }
}
