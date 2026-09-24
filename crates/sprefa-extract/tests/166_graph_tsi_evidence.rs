//! The project graph keeps TSI type facts beside their run, witness and
//! coverage, including syntax rows when a checker tier answers the same file.
#![cfg(feature = "cli")]

use std::process::Command;

fn graph(path: &str, extra: &[&str]) -> (tempfile::TempDir, rusqlite::Connection) {
    let state = tempfile::tempdir().expect("temporary state directory");
    let destination = state.path().join("graph");
    let mut command = Command::new(env!("CARGO_BIN_EXE_ryi"));
    command
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["graph", "--json", "--uses", "User", "--state"])
        .arg(&destination)
        .args(extra)
        .arg(path);
    let output = command.output().expect("graph binary runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let connection = rusqlite::Connection::open(destination.join("graph.db"))
        .expect("state store opens");
    (state, connection)
}

#[test]
fn syntax_type_evidence_retains_its_witness_and_coverage() {
    let (_state, connection) = graph("tests/fixtures/tsi/probe.rs", &[]);
    let rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM type_evidence WHERE mode = 'syntax' \
             AND method = 'parse' AND coverage = 'partial'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(rows > 0, "syntax type evidence was not landed");
}

#[cfg(feature = "rust-checker")]
#[test]
fn rust_checker_and_syntax_types_coexist_in_one_store() {
    let root = "tests/fixtures/tsi/rust_probe";
    let (_state, connection) = graph(
        "tests/fixtures/tsi/rust_probe/src/lib.rs",
        &["--project-root", root, "--rust-checker"],
    );
    let modes: Vec<String> = connection
        .prepare("SELECT DISTINCT mode FROM type_evidence ORDER BY mode")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(modes, ["semantic", "syntax"]);
}
