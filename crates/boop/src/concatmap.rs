//! The concatMap refinement loop: read new (assistant, user) contact pairs
//! from the store and pipe each through a one-shot model pass to fixed point.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};

use crate::harness::{Harness, OneShotSpec};
use crate::ident::TurnQuery;
use crate::registry::Registry;
use crate::rows::TurnRow;

/// The queue cap; past this, only the newest pair survives.
const QUEUE_CAP: usize = 4;

/// One contact pair: a user turn and the assistant turn that preceded it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pair {
    pub session: String,
    pub turn: i64,
    pub ts: i64,
    pub ai_text: String,
    pub user_text: String,
}

/// Loop configuration, one row per CLI flag.
pub struct Args {
    pub template: PathBuf,
    pub mode: String,
    pub model: String,
    pub state_dir: PathBuf,
    pub out_dir: PathBuf,
    pub poll: Duration,
    pub cap: u32,
}

/// Strip the double encoding a stored `said` can carry: a leading or trailing
/// `"` that is framing, not text.
pub fn trim_double_encoded(said: &str) -> &str {
    let said = said.strip_prefix('"').unwrap_or(said);
    said.strip_suffix('"').unwrap_or(said)
}

/// Bundle ordered rows into pairs; the mapper's own template prompts (user
/// turns opening with `mode: `) are dropped so the loop never maps itself.
pub fn bundle_pairs(rows: &[TurnRow]) -> Vec<Pair> {
    let mut pairs = Vec::new();
    let mut ai_session = String::new();
    let mut last_ai = String::new();
    for row in rows {
        let said = trim_double_encoded(&row.said).to_owned();
        if row.session != ai_session {
            // A fresh session starts with no assistant history.
            ai_session = row.session.clone();
            last_ai.clear();
        }
        if row.role == "assistant" {
            last_ai = said;
        } else if row.role == "user" && !said.starts_with("mode: ") {
            pairs.push(Pair {
                session: row.session.clone(),
                turn: row.turn,
                ts: row.ts,
                ai_text: last_ai.clone(),
                user_text: said,
            });
        }
    }
    pairs
}

/// The newest assistant turn in `pair`'s session strictly before its ts, or
/// `None` when the session had none (its first turn; nothing to rewrite).
fn last_assistant_before(store: &crate::Store, pair: &Pair) -> Option<String> {
    let query = TurnQuery {
        session: Some(pair.session.clone()),
        role: Some("assistant".to_owned()),
        until: Some(pair.ts as u64),
        ..Default::default()
    };
    let rows = store.turn_rows(&query).ok()?;
    rows.iter()
        .filter(|row| row.ts < pair.ts)
        .max_by_key(|row| row.ts)
        .map(|row| trim_double_encoded(&row.said).to_owned())
}

/// Past the cap, only the newest pair survives.
pub fn coalesce(mut pairs: Vec<Pair>) -> Vec<Pair> {
    if pairs.len() > QUEUE_CAP {
        pairs.split_off(pairs.len() - 1)
    } else {
        pairs
    }
}

/// Substitute `{{mode}}`, `{{ai_text}}`, `{{user_text}}` in the template.
/// Unknown keys stay verbatim so a typo is visible in the sent prompt.
pub fn render_template(template: &str, mode: &str, ai: &str, user: &str) -> String {
    template
        .replace("{{mode}}", mode)
        .replace("{{ai_text}}", ai)
        .replace("{{user_text}}", user)
}

/// Whitespace-collapsed form for the fixed-point test.
pub fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The resident loop.
pub fn run(args: Args) -> Result<()> {
    let store = crate::open_default().context("open boop store")?;
    let registry = Registry::discover();
    let harness = crate::lane::harness_for_model(&args.model)?
        .with_context(|| format!("model `{}` names no harness", args.model))?;
    let adapter = registry
        .by_id(&harness)
        .with_context(|| format!("no adapter registered for harness `{harness}`"))?;
    std::fs::create_dir_all(&args.state_dir)
        .with_context(|| format!("create {}", args.state_dir.display()))?;
    std::fs::create_dir_all(&args.out_dir)
        .with_context(|| format!("create {}", args.out_dir.display()))?;
    let template = std::fs::read_to_string(&args.template)
        .with_context(|| format!("read {}", args.template.display()))?;
    let mut cursor = load_or_seed_cursor(&args.state_dir, &store)?;
    let mut done = load_done(&args.state_dir)?;
    loop {
        cursor = poll_once(&store, adapter, &args, &template, cursor, &mut done)?;
        std::thread::sleep(args.poll);
    }
}

/// One tick: query new pairs, coalesce, process serially, advance the cursor.
fn poll_once(
    store: &crate::Store,
    adapter: &dyn Harness,
    args: &Args,
    template: &str,
    cursor: i64,
    done: &mut BTreeSet<(String, i64)>,
) -> Result<i64> {
    let query = TurnQuery {
        since: Some(cursor.max(0) as u64),
        ..Default::default()
    };
    let rows = store.turn_rows(&query).context("query new turns")?;
    let mut max_seen = cursor;
    for row in &rows {
        if row.ts > max_seen {
            max_seen = row.ts;
        }
    }
    let fresh: Vec<Pair> = bundle_pairs(&rows)
        .into_iter()
        .filter(|pair| !done.contains(&(pair.session.clone(), pair.turn)))
        .filter_map(|mut pair| {
            // A windowed query can miss the assistant turn that sits before
            // the cursor; pull it from the store before giving up on the pair.
            if pair.ai_text.trim().is_empty() {
                pair.ai_text = last_assistant_before(store, &pair)?;
            }
            Some(pair)
        })
        .collect();
    for pair in coalesce(fresh) {
        process_pair(&pair, adapter, args, template, done)?;
    }
    if max_seen > cursor {
        std::fs::write(args.state_dir.join("cursor"), max_seen.to_string())
            .context("write cursor")?;
    }
    Ok(max_seen)
}

/// First run seeds at the store's newest ts so only post-launch pairs map.
fn load_or_seed_cursor(state_dir: &Path, store: &crate::Store) -> Result<i64> {
    let path = state_dir.join("cursor");
    if let Ok(text) = std::fs::read_to_string(&path) {
        return text.trim().parse().with_context(|| format!("parse {}", path.display()));
    }
    let query = TurnQuery::default();
    let rows = store.turn_rows(&query).context("seed cursor")?;
    let max_ts = rows.iter().map(|row| row.ts).max().unwrap_or(0);
    std::fs::write(&path, max_ts.to_string()).context("write seed cursor")?;
    Ok(max_ts)
}

/// Done markers survive restarts: one empty file per processed (session, turn).
fn load_done(state_dir: &Path) -> Result<BTreeSet<(String, i64)>> {
    let dir = state_dir.join("done");
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let mut done = BTreeSet::new();
    for entry in std::fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
        let name = entry?.file_name();
        let name = name.to_string_lossy();
        if let Some((session, turn)) = name.rsplit_once('-') {
            if let Ok(turn) = turn.parse() {
                done.insert((session.to_owned(), turn));
            }
        }
    }
    Ok(done)
}

fn process_pair(
    pair: &Pair,
    adapter: &dyn Harness,
    args: &Args,
    template: &str,
    done: &mut BTreeSet<(String, i64)>,
) -> Result<()> {
    let msg = render_template(template, &args.mode, &pair.ai_text, &pair.user_text);
    let out_dir = args.out_dir.join(pair.session.chars().take(8).collect::<String>());
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("create {}", out_dir.display()))?;
    let out_path = out_dir.join(format!("{}.md", pair.turn));
    let text = passes_until_fixed(adapter, &msg, &args.model, args.cap)?;
    std::fs::write(&out_path, text)
        .with_context(|| format!("write {}", out_path.display()))?;
    std::fs::write(args.state_dir.join("done").join(format!("{}-{}", pair.session, pair.turn)), b"")
        .context("write done marker")?;
    done.insert((pair.session.clone(), pair.turn));
    Ok(())
}

/// One-shot passes until the normalized output repeats or the cap hits. The
/// harness adapter owns the command spelling; this loop names no binary.
fn passes_until_fixed(adapter: &dyn Harness, msg: &str, model: &str, cap: u32) -> Result<String> {
    let mut prev: Option<String> = None;
    let mut last = String::new();
    for _ in 0..cap.max(1) {
        let spec = OneShotSpec {
            model: Some(model.to_owned()),
            prompt: msg.to_owned(),
        };
        let text = adapter
            .one_shot(&spec)
            .with_context(|| format!("one-shot pass on model {model}"))?;
        if prev.as_deref() == Some(normalize(&text).as_str()) {
            return Ok(text);
        }
        prev = Some(normalize(&text));
        last = text;
    }
    Ok(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(session: &str, turn: i64, ts: i64, role: &str, said: &str) -> TurnRow {
        TurnRow {
            session: session.into(),
            harness: "claude".into(),
            turn,
            ts,
            role: role.into(),
            said: said.into(),
        }
    }

    #[test]
    fn trims_double_encoding() {
        assert_eq!(trim_double_encoded("\"hello\""), "hello");
        assert_eq!(trim_double_encoded("hello"), "hello");
    }

    #[test]
    fn bundles_user_then_assistant_into_one_pair() {
        let rows = vec![
            row("s", 1, 10, "user", "rewrite this"),
            row("s", 2, 11, "assistant", "done"),
            row("s", 3, 12, "user", "next"),
        ];
        let pairs = bundle_pairs(&rows);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].user_text, "rewrite this");
        assert_eq!(pairs[0].ai_text, "");
        assert_eq!(pairs[1].ai_text, "done");
        assert_eq!(pairs[1].user_text, "next");
    }

    #[test]
    fn mapper_template_prompts_never_bundle() {
        let rows = vec![row("m", 1, 10, "user", "mode: tighten\n\nrewrite")];
        assert!(bundle_pairs(&rows).is_empty());
    }

    #[test]
    fn coalesce_keeps_only_the_newest_past_the_cap() {
        let pairs: Vec<Pair> = (0..5)
            .map(|turn| Pair {
                session: "s".into(),
                turn,
                ts: turn,
                ai_text: String::new(),
                user_text: String::new(),
            })
            .collect();
        let kept = coalesce(pairs);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].turn, 4);
    }

    #[test]
    fn coalesce_below_the_cap_keeps_everything() {
        let pairs: Vec<Pair> = (0..4)
            .map(|turn| Pair {
                session: "s".into(),
                turn,
                ts: turn,
                ai_text: String::new(),
                user_text: String::new(),
            })
            .collect();
        assert_eq!(coalesce(pairs).len(), 4);
    }

    #[test]
    fn render_fills_all_three_keys() {
        let template = "mode: {{mode}}\n<ai>{{ai_text}}</ai>\n<user>{{user_text}}</user>";
        let rendered = render_template(template, "tighten", "a", "u");
        assert_eq!(rendered, "mode: tighten\n<ai>a</ai>\n<user>u</user>");
    }

    #[test]
    fn normalize_collapses_all_whitespace() {
        assert_eq!(normalize("a \n\t b  c"), "a b c");
    }
}
