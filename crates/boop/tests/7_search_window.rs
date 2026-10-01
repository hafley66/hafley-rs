//! `db search --days` keeps exactly the turns at or after the window start:
//! a session wholly before it drops, a session straddling it keeps its late turns.

use boop_store::testing::BoopCommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const BOOP: &str = env!("CARGO_BIN_EXE_boop");
const OLD: &str = "11111111-1111-1111-1111-111111111111";
const SPLIT: &str = "22222222-2222-2222-2222-222222222222";

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("boop-search-window-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("home")).unwrap();
    dir
}

/// One claude transcript, one user line per `(timestamp, text)`.
fn write_transcript(dir: &Path, session: &str, turns: &[(&str, &str)]) {
    let project = dir
        .join("home/.claude/projects")
        .join(format!("-tmp-boop-window-{session}"));
    std::fs::create_dir_all(&project).unwrap();
    let body: String = turns
        .iter()
        .enumerate()
        .map(|(index, (timestamp, text))| {
            format!(
                "{{\"type\":\"user\",\"uuid\":\"{session}-{index}\",\"sessionId\":\"{session}\",\
                 \"timestamp\":\"{timestamp}\",\"cwd\":\"/tmp/boop-window\",\
                 \"message\":{{\"role\":\"user\",\"content\":\"{text}\"}}}}\n"
            )
        })
        .collect();
    std::fs::write(project.join(format!("{session}.jsonl")), body).unwrap();
}

/// `(session, turn)` of each hit, in output order.
fn hits(dir: &Path, args: &[&str]) -> Vec<(String, i64)> {
    let output = Command::new(BOOP)
        .args(["db", "search", "wombat"])
        .args(args)
        .env("BOOP_DB", dir.join("boop.db"))
        .env("BOOP_MAIL_DIR", dir)
        .boop_test_root(dir.join("home"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            let row: serde_json::Value = serde_json::from_str(line).unwrap();
            (
                row["session_id"].as_str().unwrap().to_string(),
                row["turn"].as_i64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn days_window_drops_old_sessions_and_old_turns_of_live_sessions() {
    let dir = scratch();
    let recent = (time::OffsetDateTime::now_utc() - time::Duration::hours(1))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    write_transcript(&dir, OLD, &[("2020-01-01T00:00:00.000Z", "old wombat")]);
    write_transcript(
        &dir,
        SPLIT,
        &[
            ("2020-01-01T00:00:00.000Z", "early wombat"),
            (&recent, "late wombat"),
        ],
    );
    let all = hits(&dir, &["--days", "36500", "--sync"]);
    let mut all_sorted = all.clone();
    all_sorted.sort();
    assert_eq!(
        all_sorted,
        [
            (OLD.to_string(), 1),
            (SPLIT.to_string(), 1),
            (SPLIT.to_string(), 2)
        ]
    );
    assert_eq!(all[0], (SPLIT.to_string(), 2), "newest first");
    assert_eq!(hits(&dir, &["--days", "1"]), [(SPLIT.to_string(), 2)]);
    assert_eq!(
        hits(&dir, &["--days", "1", "--harness", "codex"]),
        Vec::<(String, i64)>::new()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
