//! Rust callable signature references resolve into graph type-use edges.
#![cfg(feature = "cli")]

use std::process::Command;

use serde_json::Value;

#[test]
fn rust_function_and_method_signatures_are_type_uses() {
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "graph",
            "--uses",
            "Widget",
            "tests/fixtures/graph_rust",
        ])
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
    let refs: Vec<(&str, &str)> = rows
        .iter()
        .map(|row| {
            (
                row["to_name"].as_str().unwrap(),
                row["kind"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        refs,
        [
            ("default", "param"),
            ("default", "returns"),
            ("method", "param"),
            ("method", "returns"),
            ("read", "param"),
            ("read", "returns"),
        ]
    );
}
