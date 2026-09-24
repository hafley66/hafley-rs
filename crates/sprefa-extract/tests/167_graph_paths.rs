//! Recursive graph questions return the edge rows that witness each answer.
#![cfg(feature = "cli")]

use std::process::Command;

use serde_json::Value;

fn run(arm: &str, seed: &str) -> Vec<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["graph", "--json", arm, seed, "tests/fixtures/graph_ts"])
        .output()
        .expect("graph binary runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn call_paths_carry_one_edge_id_per_hop() {
    let rows = run("--call-path", "chainA");
    let paths: Vec<(&str, u64, usize)> = rows
        .iter()
        .map(|row| {
            (
                row["to_name"].as_str().unwrap(),
                row["depth"].as_u64().unwrap(),
                row["witness"].as_array().unwrap().len(),
            )
        })
        .collect();
    assert_eq!(paths, [("chainB", 1, 1), ("chainC", 2, 2), ("chainD", 3, 3)]);
    assert!(rows.iter().all(|row| row["plane"] == "call"));
}

#[test]
fn type_paths_follow_resolved_type_edges() {
    let rows = run("--type-path", "readWidget");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["plane"], "type");
    assert_eq!(rows[0]["to_name"], "Widget");
    assert_eq!(rows[0]["depth"], 1);
    assert_eq!(rows[0]["witness"].as_array().unwrap().len(), 1);
}

#[test]
fn flow_paths_follow_derived_interprocedural_edges() {
    let files = [
        "tests/fixtures/resolve/0_caller.ts",
        "tests/fixtures/resolve/1_callee.ts",
    ];
    let reference = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["--resolve", "--family", "flow"])
        .args(files)
        .output()
        .unwrap();
    assert!(reference.status.success());
    let edge: Value = String::from_utf8(reference.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|row| row["record"] == "flow_edge")
        .expect("fixture has a flow edge");
    let seed = format!(
        "{}@{}:{}",
        edge["from_blob"].as_str().unwrap(),
        edge["from"]["start"].as_u64().unwrap(),
        edge["from"]["end"].as_u64().unwrap()
    );
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["graph", "--json", "--flow-path", &seed])
        .args(files)
        .output()
        .unwrap();
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
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|row| row["plane"] == "flow"));
    assert!(rows.iter().all(|row| row["witness"].as_array().unwrap().len()
        == row["depth"].as_u64().unwrap() as usize));
}
