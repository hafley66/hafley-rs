//! Recursive graph questions return the edge rows that witness each answer.
#![cfg(feature = "cli")]

use std::process::Command;

use serde_json::Value;

fn run(arm: &str, seed: &str) -> Vec<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["graph", arm, seed, "tests/fixtures/graph_ts"])
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
    assert_eq!(
        paths,
        [("chainB", 1, 1), ("chainC", 2, 2), ("chainD", 3, 3)]
    );
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
    let reference = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["--resolve", "--arms", "flow"])
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
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["graph", "--flow-path", &seed])
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
    assert!(rows
        .iter()
        .all(|row| row["witness"].as_array().unwrap().len()
            == row["depth"].as_u64().unwrap() as usize));
}

#[test]
fn flow_paths_from_a_parameter_match_path_and_digest_seeds() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("0_local.ts");
    let database = scratch.path().join("facts.db");
    let source = "export function local(children: string) { return children; }\n";
    std::fs::write(&path, source).unwrap();
    let start = source.find("children: string").unwrap();
    let end = start + "children: string".len();
    let seed = format!("{}@{start}:{end}", path.display());
    let path_output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(["graph", "--flow-path", &seed, "--sqlite"])
        .arg(&database)
        .arg(&path)
        .output()
        .unwrap();
    assert!(path_output.status.success(), "{}", String::from_utf8_lossy(&path_output.stderr));
    let connection = rusqlite::Connection::open(&database).unwrap();
    let digest: String = connection.query_row("SELECT digest FROM file", [], |row| row.get(0)).unwrap();
    let parameter: (u32, u32, String) = connection.query_row(
        "SELECT span__start, span__end, name FROM node WHERE family = 'df' AND kind = 'param'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).unwrap();
    assert_eq!(parameter, (start as u32, end as u32, "children".to_string()));
    let mut statement = connection.prepare(
        "SELECT to__start, to__end, min(_row) FROM edge WHERE family = 'df' \
         AND _content_id = ?1 AND from__start = ?2 AND from__end = ?3 \
         GROUP BY to__start, to__end",
    ).unwrap();
    let mut direct: Vec<(String, u64)> = statement.query_map(
        rusqlite::params![digest, start as u32, end as u32],
        |row| Ok((format!("{}:{}", row.get::<_, u32>(0)?, row.get::<_, u32>(1)?), row.get(2)?)),
    ).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    assert!(!direct.is_empty());
    direct.sort();
    let digest_output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(["graph", "--flow-path", &format!("{digest}@{start}:{end}")])
        .arg(&path)
        .output()
        .unwrap();
    assert!(digest_output.status.success(), "{}", String::from_utf8_lossy(&digest_output.stderr));
    assert_eq!(path_output.stdout, digest_output.stdout);
    let rows: Vec<Value> = String::from_utf8(path_output.stdout).unwrap().lines()
        .map(|line| serde_json::from_str(line).unwrap()).collect();
    assert!(rows.iter().all(|row| row["record"] == "graph_path"
        && row["plane"] == "flow" && row["from_path"] == digest
        && row["from_name"] == format!("{start}:{end}")
        && row["to_path"] == digest
        && row["witness"].as_array().unwrap().len() == row["depth"].as_u64().unwrap() as usize));
    let mut actual: Vec<(String, u64)> = rows.iter().filter(|row| row["depth"] == 1)
        .map(|row| (row["to_name"].as_str().unwrap().to_string(), row["witness"][0].as_u64().unwrap()))
        .collect();
    actual.sort();
    assert_eq!(actual, direct);
}

#[test]
#[cfg(feature = "graph")]
fn control_slice_returns_a_closed_statement_set() {
    let path = "tests/fixtures/graph_ts/5_slice.ts";
    let source = include_str!("fixtures/graph_ts/5_slice.ts");
    let byte = source.find("allow()").unwrap() + 1;
    let seed = format!("{path}:{byte}");
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["graph", "--slice", &seed])
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
    assert!(!rows.is_empty());
    assert!(rows
        .iter()
        .all(|row| row["record"] == "node" && row["family"] == "cfg"));
    let selected: Vec<&str> = rows
        .iter()
        .filter_map(|row| {
            let start = row["span"]["start"].as_u64()? as usize;
            let end = row["span"]["end"].as_u64()? as usize;
            source.get(start..end)
        })
        .collect();
    assert!(selected.iter().any(|text| text.contains("if (flag)")));
    assert!(selected.iter().any(|text| text.contains("allow()")));
    assert!(selected.iter().all(|text| !text.contains("after()")));
}
