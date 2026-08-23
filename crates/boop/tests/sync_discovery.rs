//! A transcript the store has never seen, written into a claude project
//! directory the store already knows, was invisible to sync: discovery keyed
//! on the mtime of `~/.claude/projects` itself, which does not move when a
//! file is created inside a child directory.

use std::path::Path;
use std::process::Command;

fn write_transcript(dir: &Path, session: &str) {
    std::fs::create_dir_all(dir).expect("make claude project dir");
    let body = format!(
        "{{\"type\":\"user\",\"uuid\":\"{session}-u\",\"sessionId\":\"{session}\",\
         \"timestamp\":\"2026-08-20T10:00:00.000Z\",\"cwd\":\"/tmp/discovery\",\
         \"message\":{{\"role\":\"user\",\"content\":\"ask\"}}}}\n\
         {{\"type\":\"assistant\",\"uuid\":\"{session}-a\",\"parentUuid\":\"{session}-u\",\
         \"sessionId\":\"{session}\",\"timestamp\":\"2026-08-20T10:00:01.000Z\",\
         \"cwd\":\"/tmp/discovery\",\"message\":{{\"role\":\"assistant\",\
         \"content\":[{{\"type\":\"text\",\"text\":\"answer\"}}]}}}}\n"
    );
    std::fs::write(dir.join(format!("{session}.jsonl")), body).expect("write transcript");
}

fn turn_count(home: &Path) -> i64 {
    let output = Command::new(env!("CARGO_BIN_EXE_boop"))
        .args([
            "db",
            "SELECT COUNT(*) AS n FROM agent_turn",
            "--format",
            "ndjson",
        ])
        .env("HOME", home)
        .env("BOOP_DB", home.join("boop.db"))
        .env("BOOP_SYNC_TRAIL", home.join("sync-trail.ndjson"))
        .output()
        .expect("run boop db");
    assert!(
        output.status.success(),
        "boop db failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("utf8 stdout");
    let line = text.lines().last().unwrap_or_default().to_owned();
    serde_json::from_str::<serde_json::Value>(&line)
        .unwrap_or_else(|error| panic!("parse {line:?}: {error}"))["n"]
        .as_i64()
        .expect("n is an integer")
}

/// RECEIPT (boop-db-convoy): failed pre-fix at 898be94 with `left: 2,
/// right: 4`. The second session was never projected.
#[test]
fn a_new_session_in_a_known_project_directory_is_discovered() {
    let home = std::env::temp_dir().join(format!("boop-discovery-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let project = home.join(".claude").join("projects").join("-tmp-discovery");
    write_transcript(&project, "11111111-1111-1111-1111-111111111111");
    assert_eq!(turn_count(&home), 2, "the first session projects");

    // Same project directory, so `~/.claude/projects` does not move.
    write_transcript(&project, "22222222-2222-2222-2222-222222222222");
    assert_eq!(
        turn_count(&home),
        4,
        "a new session beside a known one must be discovered"
    );

    let _ = std::fs::remove_dir_all(&home);
}
