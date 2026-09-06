#![cfg(feature = "agent-read")]

use boop_store::{harness_id::HarnessId, session::SessionRef, Store};
use serde_json::{json, Value};
use std::process::Command;

#[test]
fn favorite_reason_cli_roundtrip_preserves_existing_contents() {
    let dir = std::env::temp_dir().join(format!("boop-favorite-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("boop.db");
    let store = Store::open(db.clone()).unwrap();
    store
        .project_discovered_session(&SessionRef {
            harness: HarnessId::Codex,
            session_id: "reason-fixture".into(),
            nickname: "fixture".into(),
            path: dir.join("absent.jsonl"),
            cwd: None,
            git_branch: None,
            modified_ms: 1,
            size: 0,
            tmux: None,
            tmux_socket: None,
            parent: None,
        })
        .unwrap();
    store
        .write_turn(
            "reason-fixture",
            1,
            1,
            "assistant",
            "Existing assistant body",
            None,
        )
        .unwrap();
    let legacy = store
        .favorite_add("Legacy body", Some(""), "legacy", 1)
        .unwrap();
    let body = dir.join("body.md");
    std::fs::write(&body, "Pinned body\n").unwrap();
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_boop"))
            .args(args)
            .env("HOME", &dir)
            .env("BOOP_DB", &db)
            .env("BOOP_SESSION", "reason-fixture")
            .env("BOOP_NO_SYNC", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    run(&[
        "db",
        "favorite",
        "add",
        "--file",
        body.to_str().unwrap(),
        "--source",
        "omitted",
    ]);
    run(&[
        "db",
        "favorite",
        "add",
        "--file",
        body.to_str().unwrap(),
        "--source",
        "reason",
        "--note",
        "Initial reason",
    ]);
    run(&["me", "favorite"]);
    run(&["me", "favorite", "--note", "Assistant reason"]);
    let rows: Vec<Value> = run(&["db", "favorite", "list"])
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let reason = rows.iter().find(|r| r["source"] == "reason").unwrap()["favorite_id"]
        .as_i64()
        .unwrap();
    run(&[
        "db",
        "favorite",
        "edit",
        &reason.to_string(),
        "--note",
        "Edited reason",
    ]);
    let shown: Value =
        serde_json::from_str(run(&["db", "favorite", "show", &reason.to_string()]).trim()).unwrap();
    assert_eq!(
        json!([shown["body"], shown["note"], shown["source"]]),
        json!(["Pinned body\n", "Edited reason", "reason"])
    );
    let mut projection: Vec<Value> = store
        .query_favorites(None)
        .unwrap()
        .into_iter()
        .map(|row| json!([row["source"], row["body"], row["note"]]))
        .collect();
    projection.sort_by_key(Value::to_string);
    let mut expected = vec![
        json!(["legacy", "Legacy body", ""]),
        json!(["omitted", "Pinned body\n", null]),
        json!(["reason", "Pinned body\n", "Edited reason"]),
        json!([
            "codex:reason-fixture:assistant:1",
            "Existing assistant body",
            null
        ]),
        json!([
            "codex:reason-fixture:assistant:1",
            "Existing assistant body",
            "Assistant reason"
        ]),
    ];
    expected.sort_by_key(Value::to_string);
    assert_eq!(projection, expected);
    assert_eq!(
        store.query_favorite(legacy).unwrap()[0]["body"],
        "Legacy body"
    );
    drop(store);
    let reopened = Store::open(db).unwrap();
    assert_eq!(reopened.query_favorites(None).unwrap().len(), 5);
    drop(reopened);
    std::fs::remove_dir_all(dir).unwrap();
}
