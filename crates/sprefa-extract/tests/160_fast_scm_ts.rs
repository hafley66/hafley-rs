//! The TypeScript scm rows of `ryi fast`, and the wall time of a whole-corpus
//! run. No engine code is TypeScript-specific: only the query is.

#![cfg(feature = "cli")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::Value;

const ROOT: &str = "tests/fixtures/ts";

/// The lab's first whole-corpus TypeScript run hit the 10-second limit in its
/// recursive SQL traversal. Fast emits per file, so this is the rail.
const LIMIT: Duration = Duration::from_secs(10);

/// The three rows the `.scm` query owns, out of everything fast streams.
const SCM_RECORDS: [&str; 3] = ["symbol", "occurrence", "local"];

#[test]
fn the_ts_fixtures_emit_the_pinned_row_counts() {
    let (rows, _) = fast(&ts_files(ROOT), 0);
    assert_eq!(
        histogram(&rows),
        BTreeMap::from([
            ("symbol".to_string(), 85),
            ("occurrence/def".to_string(), 85),
            ("occurrence/ref".to_string(), 7),
            ("local".to_string(), 57),
        ]),
        "row counts over {ROOT}"
    );
}

/// 26 files, the corpus whose first lab run timed out.
#[test]
fn the_whole_module_plane_corpus_fits_inside_the_limit() {
    let root = "tests/fixtures/ts5_findings/module_plane";
    let files = ts_files(root);
    let (rows, elapsed) = fast(&files, 1);
    // @eprintln-ok: the measured wall time this phase reports.
    eprintln!(
        "{root}: {} scm rows in {elapsed:?} over {} files",
        rows.len(),
        files.len()
    );
    assert_eq!(
        histogram(&rows),
        BTreeMap::from([
            ("symbol".to_string(), 39),
            ("occurrence/def".to_string(), 39),
            ("occurrence/ref".to_string(), 1),
            ("local".to_string(), 18),
        ]),
        "row counts over {root}"
    );
    assert!(
        elapsed < LIMIT,
        "the whole-corpus run must fit inside {LIMIT:?}, took {elapsed:?}"
    );
}

#[test]
fn a_single_file_costs_a_fraction_of_the_limit() {
    let (rows, elapsed) = fast(&[PathBuf::from("tests/fixtures/ts/sample.ts")], 2);
    // @eprintln-ok: the measured per-file wall time this phase reports.
    eprintln!("sample.ts: {} scm rows in {elapsed:?}", rows.len());
    assert!(
        elapsed < LIMIT / 10,
        "one file must cost well under the limit, took {elapsed:?}"
    );
}

fn histogram(rows: &[Value]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for row in rows {
        let record = row["record"].as_str().expect("record tag");
        let key = match row["role"].as_str() {
            Some(role) => format!("{record}/{role}"),
            None => record.to_string(),
        };
        *counts.entry(key).or_default() += 1;
    }
    counts
}

fn ts_files(root: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect(Path::new(root), &mut files);
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

fn fast(paths: &[PathBuf], index: usize) -> (Vec<Value>, Duration) {
    let trace = std::env::temp_dir().join(format!("ryi-160-{index}-{}.json", std::process::id()));
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .arg("fast")
        .args(paths)
        .env("HAFLEY_TRACE", trace)
        .env("RUST_LOG", "sprefa_extract=debug,hafley_scm=debug")
        .output()
        .expect("ryi runs");
    let elapsed = started.elapsed();
    assert!(
        output.status.success(),
        "ryi fast {paths:?} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows = String::from_utf8(output.stdout)
        .expect("ryi emits UTF-8")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("fast emits JSON"))
        .filter(|row| {
            row["record"]
                .as_str()
                .is_some_and(|record| SCM_RECORDS.contains(&record))
        })
        .collect();
    (rows, elapsed)
}
