use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use lab_20260920_scm_locals_vs_fast::{analyze, NamedEdge};

#[test]
fn typescript_module_plane_judge() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../sprefa-extract/tests/fixtures/ts5_findings/module_plane");
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
        .env("HAFLEY_TRACE", manifest.join("traces/L5-fast.json"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let fast = output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .filter(|row| row["record"] == "resolved_edge")
        .map(|row| NamedEdge {
            caller_path: row["caller_path"].as_str().unwrap_or("").into(),
            caller_name: row["caller_name"].as_str().unwrap_or("").into(),
            callee_path: row["callee_path"].as_str().unwrap_or("").into(),
            callee_name: row["callee_name"].as_str().unwrap_or("").into(),
        })
        .collect::<BTreeSet<_>>();
    println!("ts edges both={}", lab.intersection(&fast).count());
    println!("ts edges lab-only={} {:?}", lab.difference(&fast).count(), lab.difference(&fast).collect::<Vec<_>>());
    println!("ts edges fast-only={} {:?}", fast.difference(&lab).count(), fast.difference(&lab).collect::<Vec<_>>());
}

fn collect(root: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(root)
        .unwrap()
        .map(Result::unwrap)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|part| part.to_str()) == Some("ts"))
        .collect()
}
