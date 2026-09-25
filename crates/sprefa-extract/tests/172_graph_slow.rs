//! `ryi graph --slow` walks the SCIP oracle's projection of the same tables,
//! and an oracle-answered edge grades `+`.
#![cfg(feature = "cli")]

use std::process::Command;

use serde_json::Value;

#[test]
fn slow_callers_answer_from_the_index_and_grade_plus() {
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "graph",
            "--slow",
            "--callers",
            "fetch_ref",
            "--root",
            "tests/fixtures/ratchet_soopy",
            "--scip-index",
            "tests/fixtures/ratchet_soopy/index.scip",
            "tests/fixtures/ratchet_soopy/src",
        ])
        .env("RUST_LOG", "off")
        .output()
        .expect("graph binary runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        rows,
        serde_json::from_str::<Vec<Value>>(
            r#"[{"record":"graph_edge","from_path":"tests/fixtures/ratchet_soopy/src/_13_fetch.rs","from_name":"execute_one","to_path":"tests/fixtures/ratchet_soopy/src/_13_fetch.rs","to_name":"fetch_ref","kind":"call","grade":"+","from_line":69,"to_line":76}]"#,
        )
        .unwrap()
    );
}

#[test]
fn a_zero_second_timeout_is_refused() {
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["graph", "--timeout", "0", "--callers", "x", "tests/fixtures/graph_ts"])
        .output()
        .expect("graph binary runs");
    assert_eq!(output.status.code(), Some(2));
}
