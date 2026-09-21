//! The TypeScript `scip_scm` rows through the binary, and the wall time of a
//! whole-corpus run. No engine code is TypeScript-specific: only the query is.

#![cfg(feature = "cli")]

use std::collections::BTreeMap;
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::Value;

const ROOT: &str = "tests/fixtures/ts";

/// The lab's first whole-corpus TypeScript run hit the 10-second limit in its
/// recursive SQL traversal. Pass 1 emits per file, so this is the rail.
const LIMIT: Duration = Duration::from_secs(10);

#[test]
fn the_ts_fixtures_emit_the_pinned_row_counts() {
    let (rows, _) = family(ROOT, 0);
    assert_eq!(
        histogram(&rows),
        BTreeMap::from([
            ("scip_scm_symbol".to_string(), 85),
            ("scip_scm_occurrence/def".to_string(), 85),
            ("scip_scm_occurrence/ref".to_string(), 7),
            ("scip_scm_local".to_string(), 57),
        ]),
        "row counts over {ROOT}"
    );
}

/// 26 files, the corpus whose first lab run timed out.
#[test]
fn the_whole_module_plane_corpus_fits_inside_the_limit() {
    let root = "tests/fixtures/ts5_findings/module_plane";
    let (rows, elapsed) = family(root, 1);
    let files = std::fs::read_dir(root).expect("fixture directory").count();
    // @eprintln-ok: the measured wall time this phase reports.
    eprintln!(
        "{root}: {} rows in {elapsed:?} over {files} entries",
        rows.len()
    );
    assert_eq!(
        histogram(&rows),
        BTreeMap::from([
            ("scip_scm_symbol".to_string(), 39),
            ("scip_scm_occurrence/def".to_string(), 39),
            ("scip_scm_occurrence/ref".to_string(), 1),
            ("scip_scm_local".to_string(), 18),
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
    let (rows, elapsed) = family("tests/fixtures/ts/sample.ts", 2);
    // @eprintln-ok: the measured per-file wall time this phase reports.
    eprintln!("sample.ts: {} rows in {elapsed:?}", rows.len());
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

fn family(path: &str, index: usize) -> (Vec<Value>, Duration) {
    let trace = std::env::temp_dir().join(format!(
        "ryi-160-{index}-{}.json",
        std::process::id()
    ));
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(["--family", "scip_scm", path])
        .env("HAFLEY_TRACE", trace)
        .env("RUST_LOG", "sprefa_extract=debug")
        .output()
        .expect("ryi runs");
    let elapsed = started.elapsed();
    assert!(
        output.status.success(),
        "ryi --family scip_scm {path} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows = String::from_utf8(output.stdout)
        .expect("ryi emits UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("the family emits JSON"))
        .collect();
    (rows, elapsed)
}
