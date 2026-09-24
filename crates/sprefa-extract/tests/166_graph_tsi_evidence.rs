//! The project graph keeps TSI type facts beside their run, witness and
//! coverage, including syntax rows when a checker tier answers the same file.
#![cfg(feature = "cli")]

use std::process::Command;

fn graph(path: &str, arm: (&str, &str), extra: &[&str]) -> (tempfile::TempDir, rusqlite::Connection) {
    let state = tempfile::tempdir().expect("temporary state directory");
    let destination = state.path().join("graph");
    let mut command = Command::new(env!("CARGO_BIN_EXE_ryi"));
    command
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["graph", "--json", arm.0, arm.1, "--state"])
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
    let (_state, connection) = graph("tests/fixtures/tsi/probe.rs", ("--uses", "User"), &[]);
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

#[test]
fn graph_store_retains_module_bindings_and_unresolved_rows() {
    let (_state, connection) = graph(
        "tests/fixtures/ts5_findings/module_plane",
        ("--callers", "deep"),
        &["--project-root", "tests/fixtures/ts5_findings/module_plane"],
    );
    let imports: i64 = connection
        .query_row("SELECT count(*) FROM resolved_import", [], |row| row.get(0))
        .unwrap();
    assert!(imports > 0, "resolved module bindings were dropped");
    let unresolved: i64 = connection
        .query_row("SELECT count(*) FROM unresolved", [], |row| row.get(0))
        .unwrap();
    assert!(unresolved > 0, "unresolved module references were dropped");
}

#[test]
fn scip_type_relationships_reach_the_graph_store() {
    let root = "tests/fixtures/scip_relationship";
    let (_state, connection) = graph(
        "tests/fixtures/scip_relationship/shapes.go",
        ("--uses", "Speaker"),
        &["--project-root", root, "--scip-index", "tests/fixtures/scip_relationship/index.scip"],
    );
    let pairs: Vec<(String, String)> = connection
        .prepare(
            "SELECT owner_name, target_name FROM resolved_type_edge \
             WHERE kind = 'implements' AND resolution_origin = 'scip' ORDER BY owner_name",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(pairs, [("Cat".into(), "Speaker".into()), ("Dog".into(), "Speaker".into())]);
}

#[cfg(feature = "rust-checker")]
#[test]
fn rust_checker_and_syntax_types_coexist_in_one_store() {
    let root = "tests/fixtures/tsi/rust_probe";
    let (_state, connection) = graph(
        "tests/fixtures/tsi/rust_probe/src/lib.rs",
        ("--uses", "User"),
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
