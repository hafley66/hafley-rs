//! Read verbs answer from the store with no delivery side effects, held mail
//! to a dead route stops retrying, and a search hit can be favorited.

use boop_store::testing::BoopCommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BOOP: &str = env!("CARGO_BIN_EXE_boop");
const SESSION: &str = "66666666-6666-6666-6666-666666666666";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("boop-quiet-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("home")).unwrap();
    std::fs::write(
        dir.join("registry.json"),
        r#"{"codex-dead":{"kind":"coordinator","harness":"codex","session_id":"dead-session-0000"}}"#,
    )
    .unwrap();
    dir
}

fn write_transcript(dir: &Path, session: &str, user: &str, assistant: &str) {
    let project = dir
        .join("home/.claude/projects")
        .join(format!("-tmp-boop-quiet-{session}"));
    std::fs::create_dir_all(&project).unwrap();
    let body = format!(
        "{{\"type\":\"user\",\"uuid\":\"{session}-u\",\"sessionId\":\"{session}\",\
         \"timestamp\":\"2026-08-20T10:00:00.000Z\",\"cwd\":\"/tmp/boop-quiet\",\
         \"message\":{{\"role\":\"user\",\"content\":\"{user}\"}}}}\n\
         {{\"type\":\"assistant\",\"uuid\":\"{session}-a\",\"parentUuid\":\"{session}-u\",\
         \"sessionId\":\"{session}\",\"timestamp\":\"2026-08-20T10:00:01.000Z\",\
         \"cwd\":\"/tmp/boop-quiet\",\"message\":{{\"role\":\"assistant\",\
         \"content\":[{{\"type\":\"text\",\"text\":\"{assistant}\"}}]}}}}\n"
    );
    std::fs::write(project.join(format!("{session}.jsonl")), body).unwrap();
}

fn boop(dir: &Path, args: &[&str]) -> Output {
    run(dir, args, "0")
}

/// `no_sync = "1"` is the hatch: the ledger read itself must not drain.
fn run(dir: &Path, args: &[&str], no_sync: &str) -> Output {
    let output = Command::new(BOOP)
        .args(args)
        .env("BOOP_DB", dir.join("boop.db"))
        .env("BOOP_MAIL_DIR", dir)
        .env("BOOP_NO_SYNC", no_sync)
        .boop_test_root(dir.join("home"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn send(dir: &Path, body: &str) {
    boop(
        dir,
        &[
            "mail",
            "send",
            "--to",
            "codex-dead",
            body,
            "--as",
            "quiet",
            "--no-wait",
        ],
    );
}

/// Every transition on `codex-dead`, one `<body> <outcome>` line each, in
/// send order then ledger order.
fn ledger(dir: &Path) -> String {
    let rows = stdout(&run(
        dir,
        &[
            "db",
            "SELECT m.body, t.outcome FROM agent_delivery_transition t
             JOIN agent_mail m ON m.message_id = t.message_id
             WHERE t.route = 'codex-dead' ORDER BY m.seq, t.sequence",
            "--format",
            "text",
        ],
        "1",
    ));
    rows.lines().skip(1).collect::<Vec<_>>().join("\n")
}

const SEARCH: [&str; 5] = ["db", "search", "wombat", "--days", "36500"];

#[test]
fn read_verbs_print_only_rows_and_never_drain_held_mail() {
    let dir = scratch("read");
    write_transcript(&dir, SESSION, "find the wombat", "the wombat is here");
    let synced = boop(&dir, &[&SEARCH[..], &["--sync"]].concat());
    assert_eq!(stdout(&synced).lines().count(), 2, "--sync projects first");
    send(&dir, "one");
    let before = ledger(&dir);
    assert_eq!(before, "one\tappended\none\theld-in-mailbox");

    for args in [
        &SEARCH[..],
        &["db", "sessions", "--days", "36500"],
        &["db", "lanes"],
        &["db", "mail", "codex-dead"],
        &["db", "favorite", "list"],
    ] {
        let out = stdout(&boop(&dir, args));
        assert!(
            !out.is_empty() || args[1] != "search",
            "{args:?} printed nothing"
        );
        for line in out.lines() {
            assert!(
                serde_json::from_str::<serde_json::Value>(line).is_ok(),
                "{args:?} printed a non-ndjson line: {line}"
            );
        }
    }
    let whoami = Command::new(BOOP)
        .args(["me", "whoami"])
        .env("BOOP_DB", dir.join("boop.db"))
        .env("BOOP_MAIL_DIR", &dir)
        .boop_test_root(dir.join("home"))
        .output()
        .unwrap();
    assert!(!stdout(&whoami).contains("held "), "{}", stdout(&whoami));
    assert_eq!(
        ledger(&dir),
        before,
        "a read verb walked the delivery ladder"
    );

    write_transcript(
        &dir,
        "77777777-7777-7777-7777-777777777777",
        "a second wombat",
        "late wombat",
    );
    assert_eq!(stdout(&boop(&dir, &SEARCH)).lines().count(), 2);
    assert_eq!(
        stdout(&boop(&dir, &[&SEARCH[..], &["--sync"]].concat()))
            .lines()
            .count(),
        4,
        "--sync picks up the late transcript"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn held_mail_to_a_dead_route_stops_retrying_after_the_bound() {
    let dir = scratch("dead");
    send(&dir, "one");
    send(&dir, "two");
    for _ in 0..4 {
        boop(&dir, &["db", "status"]);
    }
    assert_eq!(
        ledger(&dir),
        "one\tappended\n\
         one\theld-in-mailbox\n\
         one\theld-in-mailbox\n\
         one\theld-in-mailbox\n\
         one\troute-dead\n\
         two\tappended\n\
         two\theld-in-mailbox\n\
         two\troute-dead"
    );
    let unread = stdout(&boop(
        &dir,
        &[
            "db",
            "SELECT COUNT(*) AS n FROM agent_mail WHERE to_route = 'codex-dead' AND to_timestamp IS NULL",
            "--format",
            "text",
        ],
    ));
    assert_eq!(
        unread, "n\n2\n",
        "route-dead rows stay unread for wait --me"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_search_hit_names_the_favorite_command_and_the_command_pins_that_turn() {
    let dir = scratch("favorite");
    write_transcript(&dir, SESSION, "find the wombat", "the wombat is here");
    let text = stdout(&boop(
        &dir,
        &[&SEARCH[..], &["--sync", "--format", "text"]].concat(),
    ));
    let commands: Vec<&str> = text
        .lines()
        .filter_map(|line| line.strip_prefix('\t'))
        .collect();
    assert_eq!(
        commands,
        [
            format!("boop me favorite --session {SESSION} --turn 2"),
            format!("boop me favorite --session {SESSION} --turn 1"),
        ]
    );
    let pinned = boop(
        &dir,
        &[
            "me",
            "favorite",
            "--session",
            SESSION,
            "--turn",
            "2",
            "--note",
            "keep",
        ],
    );
    assert_eq!(stdout(&pinned), "favorite 1\n");
    let favorites = stdout(&boop(
        &dir,
        &[
            "db",
            "SELECT body, note, source FROM v_favorite",
            "--format",
            "text",
        ],
    ));
    assert_eq!(
        favorites,
        format!("body\tnote\tsource\nthe wombat is here\tkeep\tclaude:{SESSION}:assistant:2\n")
    );
    let _ = std::fs::remove_dir_all(&dir);
}
