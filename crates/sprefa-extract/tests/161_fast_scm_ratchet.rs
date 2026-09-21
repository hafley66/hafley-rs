//! Fast's ratchet: the checked-in `ts` floors, plus fast's own cross-file scope
//! graph checked against fast's resolved edges over the same files.

#![cfg(feature = "cli")]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::Value;
use sprefa_extract::{scm_edges, ResolutionOrigin};

const ROOT: &str = "tests/fixtures/ts";

/// The checked-in `ts` floors this lane must not move. `call_resolve_scip_
/// ratchet_ts` in golden_parity.rs is what charges them against real SCIP.
const TS_FLOORS: [(&str, usize); 3] = [("corpus_unique", 8), ("receiver", 1), ("scip", 2)];

/// Edges the scope graph and the resolve pass both name. A floor, not a target:
/// the two legs read the same files by different routes.
const SHARED_FLOOR: usize = 0;

/// One edge under the judge key: (caller path, caller name, callee path,
/// callee name).
type Key = (String, String, String, String);

#[test]
fn the_checked_in_ts_floors_are_where_this_lane_found_them() {
    let rows = ratchet_rows();
    for (origin, floor) in TS_FLOORS {
        let row = rows
            .iter()
            .find(|row| row.0 == "ts" && row.1 == origin)
            .unwrap_or_else(|| panic!("tests/RATCHET.tsv lost its ts/{origin} row"));
        assert_eq!(
            (row.2, row.3, row.4),
            (floor, 0, 0),
            "ts/{origin} moved: the scope graph is an input to fast, never a replacement"
        );
    }
}

#[test]
fn the_scope_graph_and_the_resolve_pass_agree_over_the_same_files() {
    let paths = ts_files();
    let fast = fast_edges(&paths);
    let scope: BTreeSet<Key> = scope_graph_edges(&paths);

    let mut before: BTreeMap<String, usize> = BTreeMap::new();
    let mut after: BTreeMap<String, usize> = BTreeMap::new();
    for (key, origin) in &fast {
        *before.entry(origin.clone()).or_default() += 1;
        if scope.contains(key) {
            *after.entry(origin.clone()).or_default() += 1;
        }
    }
    let shared = fast.keys().filter(|key| scope.contains(*key)).count();
    // @eprintln-ok: the two histograms this lane reports.
    eprintln!("fast over {ROOT}, resolved edges by origin: {before:?}");
    eprintln!("the same edges the scope graph also names: {after:?}");
    eprintln!("scope={} fast={} shared={shared}", scope.len(), fast.len());

    assert_eq!(
        ResolutionOrigin::ScmScope.as_str(),
        "scm_scope",
        "the scope-graph leg has its own closed spelling"
    );
    assert!(
        shared >= SHARED_FLOOR,
        "shared edges {shared} below the pinned floor {SHARED_FLOOR}"
    );
    let supplied: BTreeSet<String> = paths
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect();
    for key in &scope {
        assert!(
            supplied.contains(&key.0) && supplied.contains(&key.2),
            "the scope graph named a file outside the supplied set: {key:?}"
        );
    }
}

/// ONE FILE PER RUN, unioned: the lab's recorded TypeScript method. Its first
/// whole-corpus run hit the 10-second limit inside the recursive walk, and a
/// run of these 11 files reproduces that, so the corpus is never one graph.
fn scope_graph_edges(paths: &[PathBuf]) -> BTreeSet<Key> {
    let mut edges = BTreeSet::new();
    for path in paths {
        let started = Instant::now();
        let file = scm_edges(std::slice::from_ref(path)).expect("the scope graph resolves");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{}: the scope-graph resolve hit the 10-second limit",
            path.display()
        );
        edges.extend(file.into_iter().map(|edge| {
            (
                edge.caller_path,
                edge.caller_name,
                edge.callee_path,
                edge.callee_name,
            )
        }));
    }
    edges
}

fn ts_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect(Path::new(ROOT), &mut files);
    files.sort();
    files
}

fn collect(path: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).expect("fixture directory") {
        let path = entry.expect("fixture entry").path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("ts") {
            files.push(path);
        }
    }
}

/// `ryi fast` over the same file set, through the binary.
fn fast_edges(paths: &[PathBuf]) -> BTreeMap<Key, String> {
    let trace = std::env::temp_dir().join(format!("ryi-161-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .arg("fast")
        .args(paths)
        .env("HAFLEY_TRACE", trace)
        .env("RUST_LOG", "sprefa_extract=debug")
        .output()
        .expect("ryi runs");
    assert!(
        output.status.success(),
        "ryi fast failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("ryi emits UTF-8")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("fast emits JSON"))
        .filter(|row| row["record"] == "resolved_edge")
        .map(|row| {
            let text = |key: &str| row[key].as_str().unwrap_or_default().to_string();
            (
                (
                    text("caller_path"),
                    text("caller_name"),
                    text("callee_path"),
                    text("callee_name"),
                ),
                text("resolution_origin"),
            )
        })
        .collect()
}

type RatchetRow = (String, String, usize, usize, usize);

fn ratchet_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/RATCHET.tsv")
}

fn ratchet_rows() -> Vec<RatchetRow> {
    std::fs::read_to_string(ratchet_path())
        .expect("tests/RATCHET.tsv")
        .lines()
        .skip(1)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let cells: Vec<&str> = line.split('\t').collect();
            assert_eq!(cells.len(), 5, "RATCHET.tsv row needs 5 columns: {line}");
            let cell = |index: usize| cells[index].parse::<usize>().expect("RATCHET.tsv cell");
            (
                cells[0].to_string(),
                cells[1].to_string(),
                cell(2),
                cell(3),
                cell(4),
            )
        })
        .collect()
}
