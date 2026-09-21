use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use lab_20260920_scm_locals_vs_fast::{analyze, NamedEdge};

type UnresolvedKey = (String, String, String);

#[test]
fn kotlin_receiver_and_module_judge() {
    judge(
        "kotlin-receiver",
        "kotlin_receivers",
        "kt",
        Expected {
            both: &[
                "lib.kt:makeWidget -> lib.kt:Widget",
                "use.kt:ctorLeg -> lib.kt:Widget",
                "use.kt:objectLeg -> lib.kt:spin",
                "use.kt:returnLeg -> lib.kt:makeWidget",
                "use.kt:self -> use.kt:run",
            ],
            lab_only: &["use.kt:shadow -> use.kt:run"],
            fast_only: &[
                "use.kt:boundLeg -> lib.kt:project",
                "use.kt:ctorLeg -> lib.kt:run",
                "use.kt:fieldLeg -> lib.kt:run",
                "use.kt:paramLeg -> lib.kt:run",
            ],
            unresolved_both: &[],
            unresolved_lab_only: &["use.kt:project:no_graph_path", "use.kt:run:no_graph_path"],
            unresolved_fast_only: &["use.kt:run:inferred"],
        },
    );
    judge(
        "kotlin-module",
        "kotlin_module_resolve",
        "kt",
        Expected {
            both: &[
                "Main.kt:main -> Gadget.kt:lone",
                "Main.kt:main -> Gadget.kt:spin",
                "Main.kt:main -> Helper.kt:appHelper",
                "Main.kt:main -> Sibling.kt:shared",
                "Main.kt:main -> Widget.kt:makeWidget",
                "Widget.kt:makeWidget -> Widget.kt:Widget",
            ],
            lab_only: &[
                "Main.kt:main -> App.kt:dupName",
                "Main.kt:main -> Helper.kt:dupName",
                "Widget.kt:<root> -> Widget.kt:Widget",
            ],
            fast_only: &[],
            unresolved_both: &[],
            unresolved_lab_only: &[
                "Main.kt:build:no_graph_path",
                "Main.kt:println:no_graph_path",
            ],
            unresolved_fast_only: &[
                "Main.kt:dupName:ambiguous",
                "Main.kt:plus:no_corpus_def",
                "Main.kt:println:no_corpus_def",
                "Widget.kt:Widget:ambiguous",
            ],
        },
    );
}

struct Expected {
    both: &'static [&'static str],
    lab_only: &'static [&'static str],
    fast_only: &'static [&'static str],
    unresolved_both: &'static [&'static str],
    unresolved_lab_only: &'static [&'static str],
    unresolved_fast_only: &'static [&'static str],
}

fn judge(case: &str, fixture: &str, extension: &str, expected: Expected) {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .join("../sprefa-extract/tests/fixtures")
        .join(fixture);
    let mut paths = collect(&root, extension);
    paths.sort();
    let query = std::fs::read_to_string(manifest.join("queries/kotlin/locals.scm")).unwrap();
    let lab = analyze("kotlin", &query, &paths).unwrap();
    let ryi = ryi_bin();
    let mut command = Command::new("timeout");
    command.arg("10").arg(ryi).arg("fast").args(&paths).env(
        "HAFLEY_TRACE",
        manifest.join(format!("traces/L4-{case}-fast.json")),
    );
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let lab_edges = lab.edges.into_iter().collect::<BTreeSet<_>>();
    let lab_unresolved = lab
        .unresolved
        .into_iter()
        .map(|row| (row.path, row.name, row.reason))
        .collect::<BTreeSet<_>>();
    let (fast_edges, fast_unresolved) = fast_rows(&output.stdout);
    assert_split(
        case,
        "edges",
        &lab_edges,
        &fast_edges,
        expected.both,
        expected.lab_only,
        expected.fast_only,
        edge_text,
    );
    assert_split(
        case,
        "unresolved",
        &lab_unresolved,
        &fast_unresolved,
        expected.unresolved_both,
        expected.unresolved_lab_only,
        expected.unresolved_fast_only,
        unresolved_text,
    );
}

fn ryi_bin() -> PathBuf {
    let target = PathBuf::from(std::env::var("CARGO_TARGET_DIR").expect("CARGO_TARGET_DIR"));
    assert!(!target.to_string_lossy().contains("/.cache/boop/"));
    target.join("debug/ryi")
}

fn fast_rows(bytes: &[u8]) -> (BTreeSet<NamedEdge>, BTreeSet<UnresolvedKey>) {
    let mut edges = BTreeSet::new();
    let mut unresolved = BTreeSet::new();
    for line in bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
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

fn assert_split<T: Ord>(
    case: &str,
    plane: &str,
    lab: &BTreeSet<T>,
    fast: &BTreeSet<T>,
    expected_both: &[&str],
    expected_lab_only: &[&str],
    expected_fast_only: &[&str],
    text: impl Fn(&T) -> String,
) {
    let both = lab.intersection(fast).map(&text).collect::<BTreeSet<_>>();
    let lab_only = lab.difference(fast).map(&text).collect::<BTreeSet<_>>();
    let fast_only = fast.difference(lab).map(&text).collect::<BTreeSet<_>>();
    assert_eq!(
        both,
        expected_both
            .iter()
            .map(|value| value.to_string())
            .collect()
    );
    assert_eq!(
        lab_only,
        expected_lab_only
            .iter()
            .map(|value| value.to_string())
            .collect()
    );
    assert_eq!(
        fast_only,
        expected_fast_only
            .iter()
            .map(|value| value.to_string())
            .collect()
    );
    println!("{case} {plane} both={} {both:?}", both.len());
    println!("{case} {plane} lab-only={} {lab_only:?}", lab_only.len());
    println!("{case} {plane} fast-only={} {fast_only:?}", fast_only.len());
}

fn edge_text(edge: &NamedEdge) -> String {
    format!(
        "{}:{} -> {}:{}",
        file(&edge.caller_path),
        edge.caller_name,
        file(&edge.callee_path),
        edge.callee_name
    )
}

fn unresolved_text(row: &UnresolvedKey) -> String {
    format!("{}:{}:{}", file(&row.0), row.1, row.2)
}

fn file(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|part| part.to_str())
        .unwrap_or(path)
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
