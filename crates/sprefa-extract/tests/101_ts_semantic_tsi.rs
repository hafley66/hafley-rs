//! Stock tsgo semantic rows and demand-scoped coverage.
#![cfg(feature = "ts-checker")]
use std::process::Command;
use serde_json::Value;

fn stream(file: &str) -> Vec<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["--witness", "--resolve", "--arms", "type", "--root",
            "tests/fixtures/tsi", "--ts-checker", file])
        .output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().lines()
        .map(|line| serde_json::from_str(line).unwrap()).collect()
}

#[test]
fn stock_semantic_output_and_coverage() {
    let rows = stream("tests/fixtures/tsi/probe.ts");
    assert!(rows.iter().any(|row| row["record"] == "run" && row["tool"] == "tsgo"));
    for relation in ["tsi.subtype", "tsi.equivalent"] {
        assert!(!rows.iter().any(|row| row["record"] == "fact" && row["relation"] == relation));
        assert!(rows.iter().any(|row| row["record"] == "coverage" && row["run"] == 1
            && row["relation"] == relation && row["coverage"] == "partial"));
        assert!(rows.iter().any(|row| row["record"] == "diagnostic" && row["run"] == 1
            && row["relation"] == relation));
    }
    insta::assert_json_snapshot!("stock_semantic_output", rows);
}

#[test]
fn recursive_stock_types_close_their_ids() {
    let rows = stream("tests/fixtures/tsi/recursive.ts");
    insta::assert_json_snapshot!("stock_recursive_output", rows);
}
