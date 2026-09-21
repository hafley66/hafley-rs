use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use lab_20260920_scm_locals_vs_fast::{analyze, NamedEdge};

type UnresolvedKey = (String, String, String);

#[test]
fn kotlin_receiver_and_module_judge() {
    judge("kotlin-receiver", "kotlin_receivers", "kt");
    judge("kotlin-module", "kotlin_module_resolve", "kt");
}

fn judge(case: &str, fixture: &str, extension: &str) {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../sprefa-extract/tests/fixtures").join(fixture);
    let mut paths = collect(&root, extension);
    paths.sort();
    let query = std::fs::read_to_string(manifest.join("queries/kotlin/locals.scm")).unwrap();
    let lab = analyze("kotlin", &query, &paths).unwrap();
    let ryi = std::env::var("RYI_BIN")
        .unwrap_or_else(|_| "/Users/chrishafley/.cache/boop/cargo-target/debug/ryi".into());
    let mut command = Command::new(ryi);
    command.arg("fast").args(&paths).env(
        "HAFLEY_TRACE",
        manifest.join(format!("traces/L4-{case}-fast.json")),
    );
    let output = command.output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let lab_edges = lab.edges.into_iter().collect::<BTreeSet<_>>();
    let lab_unresolved = lab
        .unresolved
        .into_iter()
        .map(|row| (row.path, row.name, row.reason))
        .collect::<BTreeSet<_>>();
    let (fast_edges, fast_unresolved) = fast_rows(&output.stdout);
    print_split(case, "edges", &lab_edges, &fast_edges);
    print_split(case, "unresolved", &lab_unresolved, &fast_unresolved);
}

fn fast_rows(bytes: &[u8]) -> (BTreeSet<NamedEdge>, BTreeSet<UnresolvedKey>) {
    let mut edges = BTreeSet::new();
    let mut unresolved = BTreeSet::new();
    for line in bytes.split(|byte| *byte == b'\n').filter(|line| !line.is_empty()) {
        let row: serde_json::Value = serde_json::from_slice(line).unwrap();
        match row["record"].as_str() {
            Some("resolved_edge") => {
                edges.insert(NamedEdge {
                    caller_path: row["caller_path"].as_str().unwrap_or("").into(),
                    caller_name: row["caller_name"].as_str().unwrap_or("").into(),
                    callee_path: row["callee_path"].as_str().unwrap_or("").into(),
                    callee_name: row["callee_name"].as_str().unwrap_or("").into(),
                });
            }
            Some("unresolved") if row["family"] == "call" => {
                unresolved.insert((
                    row["path"].as_str().unwrap_or("").into(),
                    row["detail"].as_str().unwrap_or("").into(),
                    row["reason"].as_str().unwrap_or("").into(),
                ));
            }
            _ => {}
        }
    }
    (edges, unresolved)
}

fn print_split<T: Ord + std::fmt::Debug>(case: &str, plane: &str, lab: &BTreeSet<T>, fast: &BTreeSet<T>) {
    let both = lab.intersection(fast).collect::<Vec<_>>();
    let lab_only = lab.difference(fast).collect::<Vec<_>>();
    let fast_only = fast.difference(lab).collect::<Vec<_>>();
    println!("{case} {plane} both={} {both:?}", both.len());
    println!("{case} {plane} lab-only={} {lab_only:?}", lab_only.len());
    println!("{case} {plane} fast-only={} {fast_only:?}", fast_only.len());
}

fn collect(root: &Path, extension: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(root).unwrap().map(Result::unwrap) {
        let path = entry.path();
        if path.is_dir() {
            paths.extend(collect(&path, extension));
        } else if path.extension().and_then(|part| part.to_str()) == Some(extension) {
            paths.push(path);
        }
    }
    paths
}
