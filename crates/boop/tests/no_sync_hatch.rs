//! `BOOP_NO_SYNC=1` skips the startup transcript sync and still answers from
//! the store. The receipt is a turn COUNT, never a wall reading: a transcript
//! appended between two runs changes the count only on the run that synced.

use std::path::Path;
use std::process::Command;

/// Two records per transcript, one user turn and one assistant turn. Each
/// session gets its own project directory: discovery of a path the store has
/// never seen keys on the mtime of `~/.claude/projects` itself
/// (`crates/boop/src/cli/db.rs:200` `root_stamps_match`), which only a new
/// child directory moves.
fn write_transcript(home: &Path, session: &str) {
    let dir = home
        .join(".claude")
        .join("projects")
        .join(format!("-tmp-boop-nosync-{session}"));
    std::fs::create_dir_all(&dir).expect("make claude project dir");
    let body = format!(
        "{{\"type\":\"user\",\"uuid\":\"{session}-u\",\"sessionId\":\"{session}\",\
         \"timestamp\":\"2026-08-20T10:00:00.000Z\",\"cwd\":\"/tmp/boop-nosync\",\
         \"message\":{{\"role\":\"user\",\"content\":\"ask\"}}}}\n\
         {{\"type\":\"assistant\",\"uuid\":\"{session}-a\",\"parentUuid\":\"{session}-u\",\
         \"sessionId\":\"{session}\",\"timestamp\":\"2026-08-20T10:00:01.000Z\",\
         \"cwd\":\"/tmp/boop-nosync\",\"message\":{{\"role\":\"assistant\",\
         \"content\":[{{\"type\":\"text\",\"text\":\"answer\"}}]}}}}\n"
    );
    std::fs::write(dir.join(format!("{session}.jsonl")), body).expect("write transcript");
}

/// The turn count the passthrough reports, through a boop subprocess whose
/// HOME and BOOP_DB both point at the fixture.
fn turn_count(home: &Path, no_sync: bool) -> i64 {
    let mut command = Command::new(env!("CARGO_BIN_EXE_boop"));
    command
        .args([
            "db",
            "SELECT COUNT(*) AS n FROM agent_turn",
            "--format",
            "ndjson",
        ])
        .env("HOME", home)
        .env("BOOP_DB", home.join("boop.db"));
    if no_sync {
        command.env("BOOP_NO_SYNC", "1");
    } else {
        command.env_remove("BOOP_NO_SYNC");
    }
    let output = command.output().expect("run boop db");
    assert!(
        output.status.success(),
        "boop db failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("utf8 stdout");
    let line = text.lines().last().unwrap_or_default();
    let row: serde_json::Value =
        serde_json::from_str(line).unwrap_or_else(|error| panic!("parse {line:?}: {error}"));
    row["n"].as_i64().expect("n is an integer")
}

#[test]
fn the_no_sync_hatch_skips_the_startup_sync_and_still_reads_rows() {
    let home = std::env::temp_dir().join(format!("boop-nosync-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("make temp home");

    write_transcript(&home, "11111111-1111-1111-1111-111111111111");
    let seeded = turn_count(&home, false);
    assert_eq!(seeded, 2, "the first run projects the seeded transcript");

    write_transcript(&home, "22222222-2222-2222-2222-222222222222");
    let hatched = turn_count(&home, true);
    assert_eq!(
        hatched, seeded,
        "BOOP_NO_SYNC=1 must not project the appended transcript"
    );
    assert!(hatched > 0, "the hatch still answers from the store");

    let defaulted = turn_count(&home, false);
    assert_eq!(
        defaulted, 4,
        "the default path must still sync, or the hatch has become the default"
    );

    let _ = std::fs::remove_dir_all(&home);
}
