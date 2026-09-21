//! `ryi graph --state DIR` leaves a store whose three views answer by name:
//! the arm's own rows are one `SELECT` a caller can repeat by hand.
#![cfg(feature = "cli")]

use std::path::PathBuf;
use std::process::Command;

fn scratch(tag: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "sprefa_graph_views_{tag}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn count(connection: &rusqlite::Connection, sql: &str) -> i64 {
    connection.query_row(sql, [], |row| row.get(0)).unwrap()
}

#[test]
fn state_store_carries_the_three_graph_views() {
    let scratch = scratch("state");
    let state = scratch.join("store");
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(["graph", "--json", "--callers", "deep", "--state"])
        .arg(&state)
        .arg("tests/fixtures/ts5_findings/module_plane")
        .env("HAFLEY_TRACE", scratch.join("graph-views.json"))
        .env("RUST_LOG", "off")
        .output()
        .expect("graph binary runs");
    assert!(
        output.status.success(),
        "graph failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap().lines().count(), 1);

    let connection = rusqlite::Connection::open(state.join("graph.db")).unwrap();
    let views: Vec<String> = connection
        .prepare("SELECT name FROM sqlite_master WHERE type = 'view' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert!(
        views.iter().any(|name| name == "callers")
            && views.iter().any(|name| name == "uses")
            && views.iter().any(|name| name == "reach"),
        "{views:?}"
    );

    // `callers` is a plain projection: one row per resolved_edge, no filter.
    // `uses` is empty because --callers runs the call arm and no other.
    assert_eq!(count(&connection, "SELECT count(*) FROM resolved_edge"), 9);
    assert_eq!(count(&connection, "SELECT count(*) FROM callers"), 9);
    assert_eq!(count(&connection, "SELECT count(*) FROM uses"), 0);
    assert_eq!(count(&connection, "SELECT count(*) FROM reach"), 9);
    assert_eq!(
        count(
            &connection,
            "SELECT count(*) FROM callers WHERE callee_name = 'deep'"
        ),
        1
    );
    assert_eq!(
        count(
            &connection,
            "SELECT count(*) FROM callers WHERE grade NOT IN ('+', '~', '-')"
        ),
        0
    );
    assert_eq!(count(&connection, "SELECT max(depth) FROM reach"), 1);
}

#[test]
fn the_memory_store_leaves_no_file_behind() {
    let scratch = scratch("memory");
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(["graph", "--json", "--callers", "deep"])
        .arg("tests/fixtures/ts5_findings/module_plane")
        .env("HAFLEY_TRACE", scratch.join("graph-memory.json"))
        .env("RUST_LOG", "off")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("graph binary runs");
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().lines().count(), 1);
    assert!(!PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("graph.db")
        .exists());
}
