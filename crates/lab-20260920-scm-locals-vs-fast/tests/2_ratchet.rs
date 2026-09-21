use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use lab_20260920_scm_locals_vs_fast::{analyze, NamedEdge};

#[test]
fn lab_edges_against_checked_in_ts_floors() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../sprefa-extract/tests/fixtures/ts");
    let mut paths = collect(&root);
    paths.sort();
    let query = std::fs::read_to_string(manifest.join("queries/typescript/locals.scm")).unwrap();
    let lab = paths
        .iter()
        .flat_map(|path| analyze("ts", &query, std::slice::from_ref(path)).unwrap().edges)
        .collect::<BTreeSet<_>>();

    let ryi = std::env::var("RYI_BIN")
        .unwrap_or_else(|_| "/Users/chrishafley/.cache/boop/cargo-target/debug/ryi".into());
    let output = Command::new(ryi)
        .arg("fast")
        .args(&paths)
        .env("HAFLEY_TRACE", manifest.join("traces/L6-fast.json"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let mut fast = BTreeMap::new();
    for line in output.stdout.split(|byte| *byte == b'\n').filter(|line| !line.is_empty()) {
        let row: serde_json::Value = serde_json::from_slice(line).unwrap();
        if row["record"] != "resolved_edge" {
            continue;
        }
        fast.insert(
            NamedEdge {
                caller_path: row["caller_path"].as_str().unwrap_or("").into(),
                caller_name: row["caller_name"].as_str().unwrap_or("").into(),
                callee_path: row["callee_path"].as_str().unwrap_or("").into(),
                callee_name: row["callee_name"].as_str().unwrap_or("").into(),
            },
            row["resolution_origin"].as_str().unwrap_or("").to_string(),
        );
    }

    let mut actual: BTreeMap<String, usize> = BTreeMap::new();
    for edge in &lab {
        if let Some(origin) = fast.get(edge) {
            *actual.entry(origin.clone()).or_default() += 1;
        }
    }
    let rows = std::fs::read_to_string(manifest.join("../sprefa-extract/tests/RATCHET.tsv")).unwrap();
    for line in rows.lines().skip(1).filter(|line| line.starts_with("ts\t")) {
        let cells = line.split('\t').collect::<Vec<_>>();
        let floor = cells[2].parse::<usize>().unwrap();
        let count = actual.get(cells[1]).copied().unwrap_or_default();
        println!("ts/{} true={} floor={} holds={}", cells[1], count, floor, count >= floor);
    }
    println!("lab={} fast={} shared={}", lab.len(), fast.len(), lab.iter().filter(|edge| fast.contains_key(*edge)).count());
}

fn collect(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(root).unwrap().map(Result::unwrap) {
        let path = entry.path();
        if path.is_dir() {
            paths.extend(collect(&path));
        } else if path.extension().and_then(|part| part.to_str()) == Some("ts") {
            paths.push(path);
        }
    }
    paths
}
