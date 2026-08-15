//! The runtime loop: read visible turns from the boop store, bundle them into
//! pairs, route each through the host, drive the resident chat via the
//! interpreter, tail the reply, fold it into state, commit, and advance the
//! cursor. The pure pieces (bundling, fold-and-commit, transcript tailing) are
//! unit tested here; the store and tmux wiring is thin.

use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result};

use crate::action::Action;
use crate::host::{Host, Pair};
use crate::interp::Effects;
use crate::interp::ChatSink;

/// Strip the v0 double-encoding: a stored `said` may carry a leading `"` that
/// is part of the framing, not the text.
pub fn trim_double_encoded(said: &str) -> &str {
    let said = said.strip_prefix('"').unwrap_or(said);
    let said = said.strip_suffix('"').unwrap_or(said);
    said
}

/// Bundle ordered turn rows into user/assistant pairs. A `user` turn opens a
/// pair; the following assistant (or tool) turn supplies its `ai_text`; the
/// next `user` turn closes the prior pair. The last open pair is closed at the
/// end.
pub fn bundle_pairs(turns: &[boop::rows::TurnRow]) -> Vec<Pair> {
    let mut pairs = Vec::new();
    let mut open: Option<Pair> = None;
    for turn in turns {
        let said = trim_double_encoded(&turn.said).to_owned();
        if turn.role == "user" {
            if let Some(prior) = open.take() {
                pairs.push(prior);
            }
            open = Some(Pair {
                session: turn.session.clone(),
                turn: turn.turn,
                ai_text: String::new(),
                user_text: said,
            });
        } else if let Some(prior) = open.as_mut() {
            if prior.ai_text.is_empty() {
                prior.ai_text = said;
            }
        }
    }
    if let Some(prior) = open {
        pairs.push(prior);
    }
    pairs
}

/// Query the store for turns of `session` with `ts` strictly greater than the
/// cursor.
pub fn read_new_turns(
    store: &boop::Store,
    session: &str,
    since: i64,
) -> Result<Vec<boop::rows::TurnRow>> {
    let query = boop::ident::TurnQuery {
        session: Some(session.to_owned()),
        since: Some(since.max(0) as u64),
        ..Default::default()
    };
    store.turn_rows(&query).context("query new turns")
}

/// Read the complete lines of a transcript forward from a byte offset. Returns
/// the accumulated text and the offset to resume from. A trailing partial line
/// is left untouched (the partial-line law from the boop tailer).
pub fn tail_transcript(path: &Path, from: u64) -> Result<(String, u64)> {
    let mut file = File::open(path)
        .with_context(|| format!("open transcript {}", path.display()))?;
    let result = boop::tail::read_complete_lines(&mut file, from)
        .with_context(|| format!("tail transcript {}", path.display()))?;
    let mut text = String::new();
    for line in &result.lines {
        text.push_str(&String::from_utf8_lossy(&line.bytes));
        text.push('\n');
    }
    Ok((text, result.next_offset))
}

/// Fold a completed reply into state and commit when it changed. Returns true
/// when a commit was written. `note` is the commit subject for this fold.
pub fn fold_and_commit<S: ChatSink>(
    host: &mut Host,
    effects: &Effects<S>,
    reply: &str,
    note: &str,
) -> Result<bool> {
    for action in host.complete_reply(reply) {
        effects.apply(&action)?;
    }
    if host.flush()? {
        effects.apply(&Action::Commit {
            path: host.state_path().clone(),
            note: note.to_owned(),
        })?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use crate::cursor::Cursor;
    use crate::host::Host;
    use crate::interp::Effects;
    use crate::pipe::{bundle_pairs, fold_and_commit, tail_transcript, trim_double_encoded};
    use crate::rules::RuleSet;

    fn turn(session: &str, turn: i64, role: &str, said: &str) -> boop::rows::TurnRow {
        boop::rows::TurnRow {
            session: session.into(),
            harness: "claude".into(),
            turn,
            ts: turn,
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
            turn("s", 1, "user", "rewrite this"),
            turn("s", 2, "assistant", "done"),
        ];
        let pairs = bundle_pairs(&rows);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].user_text, "rewrite this");
        assert_eq!(pairs[0].ai_text, "done");
    }

    #[test]
    fn trailing_user_turn_is_closed_as_an_empty_ai_pair() {
        let rows = vec![turn("s", 1, "user", "hello")];
        let pairs = bundle_pairs(&rows);
        assert_eq!(pairs.len(), 1);
        assert!(pairs[0].ai_text.is_empty());
    }

    #[test]
    fn tail_transcript_reads_complete_lines_only() {
        let path = std::env::temp_dir().join(format!(
            "concatmap_tail_{}_{}",
            std::process::id(),
            "t1"
        ));
        std::fs::write(&path, "a\nb\n").unwrap();
        let (text, next) = tail_transcript(&path, 0).unwrap();
        assert_eq!(text, "a\nb\n");
        assert_eq!(next, 4);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fold_and_commit_commits_once_and_idempotent_replay_does_not() {
        let repo = std::env::temp_dir().join(format!(
            "concatmap_fc_{}_{}",
            std::process::id(),
            "repo"
        ));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).unwrap();
        let cmds: &[&[&str]] = &[
            &["init", "-q"],
            &["config", "user.email", "pipe@local"],
            &["config", "user.name", "pipe"],
        ];
        for cmd in cmds {
            let status = std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(*cmd)
                .status()
                .unwrap();
            assert!(status.success());
        }
        let state_path = repo.join("state/agent.dl6");
        let mut host = Host::new(
            "tighten",
            RuleSet::default(),
            state_path.clone(),
        )
        .unwrap();
        let sink = crate::interp::tests::RecordingSink::default();
        let effects = Effects {
            sink,
            worktree: repo.clone(),
        };
        // First fold changes state and commits.
        let changed = fold_and_commit(
            &mut host,
            &effects,
            "fact state_note(agent=\"tighten\", key=\"k\", body=\"v\")",
            "fold",
        )
        .unwrap();
        assert!(changed);
        // Replaying the same delta produces no second commit.
        let changed_again = fold_and_commit(
            &mut host,
            &effects,
            "fact state_note(agent=\"tighten\", key=\"k\", body=\"v\")",
            "fold",
        )
        .unwrap();
        assert!(!changed_again, "idempotent replay must not commit");
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn cursor_is_outside_the_fold() {
        let mut cursor = Cursor::default();
        assert!(cursor.observe(3));
        assert_eq!(cursor.max_ts, 3);
    }
}
