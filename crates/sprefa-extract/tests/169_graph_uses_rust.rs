//! Rust callable signature references resolve into graph type-use edges.
#![cfg(feature = "cli")]

use std::process::Command;

use serde_json::Value;

#[test]
fn rust_function_and_method_signatures_are_type_uses() {
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["graph", "--uses", "Widget", "tests/fixtures/graph_rust_crate"])
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
                row["from_name"].as_str().unwrap(),
                row["kind"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        refs,
        [
            ("default", "param"),
            ("default", "returns"),
            ("make", "returns"),
            ("make", "uses"),
            ("method", "param"),
            ("method", "returns"),
            ("read", "param"),
            ("read", "returns"),
        ]
    );
}

#[test]
fn graph_uses_follows_corpus_crate_reexports_and_declines_registry_crates() {
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/graph_uses_rust");
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args([
            "graph",
            "--uses",
            "Widget",
            "--root",
            fixtures.to_str().unwrap(),
        ])
        .arg(fixtures.join("path_case"))
        .output()
        .expect("graph binary runs for path dependency");
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
    assert!(
        rows.iter().any(|row| {
            row["record"] == "graph_edge"
                && row["from_path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with("client/src/lib.rs"))
                && row["to_path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with("bridge/src/model.rs"))
                && row["to_name"] == "Widget"
        }),
        "expected the path dependency re-export to resolve: {rows:#?}"
    );

    let external = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args([
            "graph",
            "--uses",
            "Widget",
            "--root",
            fixtures.to_str().unwrap(),
        ])
        .arg(fixtures.join("external_case/client"))
        .arg(fixtures.join("path_case/bridge"))
        .output()
        .expect("graph binary runs for registry dependency");
    assert!(
        external.status.success(),
        "{}",
        String::from_utf8_lossy(&external.stderr)
    );
    let rows: Vec<Value> = String::from_utf8(external.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        rows,
        [serde_json::json!({
            "record": "graph_decline",
            "from_path": fixtures.join("external_case/client/src/lib.rs").to_string_lossy(),
            "from_name": "make",
            "type_name": "Widget",
            "crate_name": "external_widgets",
            "reason": "external_crate",
            "kind": "returns"
        })]
    );
}
