//! The Kotlin scm rows of `ryi fast`: counted, and per-file pure.

#![cfg(feature = "cli")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const FIXTURES: [&str; 2] = [
    "tests/fixtures/kotlin_receivers",
    "tests/fixtures/kotlin_module_resolve",
];

/// The three rows the `.scm` query owns. Fast's resolved rows ride the same
/// stream and are counted by their own tests.
const SCM_RECORDS: [&str; 3] = ["symbol", "occurrence", "local"];

#[test]
fn the_kotlin_fixtures_emit_the_pinned_row_counts() {
    let rows = scm_rows(&kotlin_files(), 0);
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for row in &rows {
        let record = row["record"].as_str().expect("record tag");
        let key = match row["role"].as_str() {
            Some(role) => format!("{record}/{role}"),
            None => record.to_string(),
        };
        *counts.entry(key).or_default() += 1;
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            ("symbol".to_string(), 47),
            ("occurrence/def".to_string(), 47),
            ("occurrence/ref".to_string(), 5),
            ("local".to_string(), 20),
        ]),
        "row counts over {FIXTURES:?}"
    );
}

#[test]
fn every_symbol_carries_the_lab_spelling_and_a_defining_occurrence() {
    let rows = scm_rows(&kotlin_files(), 1);
    let defs: Vec<&Value> = rows
        .iter()
        .filter(|row| row["record"] == "occurrence" && row["role"] == "def")
        .collect();
    for row in rows.iter().filter(|row| row["record"] == "symbol") {
        let symbol = row["symbol"].as_str().expect("symbol");
        let path = row["path"].as_str().expect("path");
        assert!(
            symbol.starts_with(&format!("scm . . `{path}`/")) && symbol.ends_with("()."),
            "symbol spelling: {symbol}"
        );
        assert!(
            defs.iter().any(|def| def["symbol"] == row["symbol"]),
            "every symbol has a def occurrence: {symbol}"
        );
    }
}

#[test]
fn a_reference_names_a_symbol_the_same_file_defines() {
    let rows = scm_rows(&kotlin_files(), 2);
    for row in rows
        .iter()
        .filter(|row| row["record"] == "occurrence" && row["role"] == "ref")
    {
        assert!(
            rows.iter().any(|other| other["record"] == "symbol"
                && other["symbol"] == row["symbol"]
                && other["path"] == row["path"]),
            "the scm rows resolve inside one file only: {row}"
        );
    }
}

#[test]
fn the_rows_of_one_file_do_not_depend_on_the_other_files() {
    let whole = scm_rows(&kotlin_files(), 3);
    let mut apart = Vec::new();
    for (index, file) in kotlin_files().into_iter().enumerate() {
        apart.extend(scm_rows(&[file], 10 + index));
    }
    let key = |row: &Value| serde_json::to_string(row).expect("a row is serializable");
    let mut whole: Vec<String> = whole.iter().map(key).collect();
    let mut apart: Vec<String> = apart.iter().map(key).collect();
    whole.sort();
    apart.sort();
    assert_eq!(whole, apart, "per-file purity");
}

fn kotlin_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for fixture in FIXTURES {
        collect(Path::new(fixture), &mut files);
    }
    files.sort();
    files
}

fn collect(path: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).expect("fixture directory") {
        let path = entry.expect("fixture entry").path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("kt") {
            files.push(path);
        }
    }
}

/// `ryi fast` over the supplied files, narrowed to the rows the `.scm` owns.
fn scm_rows(paths: &[PathBuf], index: usize) -> Vec<Value> {
    let trace = std::env::temp_dir().join(format!("ryi-158-{index}-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .arg("fast")
        .args(paths)
        .env("HAFLEY_TRACE", trace)
        .env("RUST_LOG", "sprefa_extract=debug")
        .output()
        .expect("ryi runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "ryi fast {paths:?} failed:\n{stderr}"
    );
    String::from_utf8(output.stdout)
        .expect("ryi emits UTF-8")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("fast emits JSON"))
        .filter(|row| {
            row["record"]
                .as_str()
                .is_some_and(|record| SCM_RECORDS.contains(&record))
        })
        .collect()
}
