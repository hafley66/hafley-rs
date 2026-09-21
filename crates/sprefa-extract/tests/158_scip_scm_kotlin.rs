//! The Kotlin `scip_scm` rows through the binary: counted, and per-file pure.

#![cfg(feature = "cli")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const FIXTURES: [&str; 2] = [
    "tests/fixtures/kotlin_receivers",
    "tests/fixtures/kotlin_module_resolve",
];

#[test]
fn the_kotlin_fixtures_emit_the_pinned_row_counts() {
    let rows = family(&FIXTURES.map(PathBuf::from), 0);
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
            ("scip_scm_symbol".to_string(), 47),
            ("scip_scm_occurrence/def".to_string(), 47),
            ("scip_scm_occurrence/ref".to_string(), 5),
            ("scip_scm_local".to_string(), 20),
        ]),
        "row counts over {FIXTURES:?}"
    );
}

#[test]
fn every_symbol_carries_the_lab_spelling_and_a_defining_occurrence() {
    let rows = family(&FIXTURES.map(PathBuf::from), 1);
    let defs: Vec<&Value> = rows
        .iter()
        .filter(|row| row["record"] == "scip_scm_occurrence" && row["role"] == "def")
        .collect();
    for row in rows.iter().filter(|row| row["record"] == "scip_scm_symbol") {
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
    let rows = family(&FIXTURES.map(PathBuf::from), 2);
    for row in rows
        .iter()
        .filter(|row| row["record"] == "scip_scm_occurrence" && row["role"] == "ref")
    {
        assert!(
            rows.iter().any(|other| other["record"] == "scip_scm_symbol"
                && other["symbol"] == row["symbol"]
                && other["path"] == row["path"]),
            "pass 1 resolves inside one file only: {row}"
        );
    }
}

#[test]
fn the_rows_of_one_file_do_not_depend_on_the_other_files() {
    let whole = family(&FIXTURES.map(PathBuf::from), 3);
    let mut apart = Vec::new();
    for (index, file) in kotlin_files().into_iter().enumerate() {
        apart.extend(family(&[file], 10 + index));
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

fn family(paths: &[PathBuf], index: usize) -> Vec<Value> {
    let trace = std::env::temp_dir().join(format!(
        "ryi-158-{index}-{}.json",
        std::process::id()
    ));
    let mut command = Command::new(env!("CARGO_BIN_EXE_ryi"));
    command
        .args(["--family", "scip_scm"])
        .args(paths)
        .env("HAFLEY_TRACE", trace)
        .env("RUST_LOG", "sprefa_extract=debug");
    let output = command.output().expect("ryi runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "ryi --family scip_scm {paths:?} failed:\n{stderr}"
    );
    assert!(
        !stderr.contains("ScmLowerError"),
        "the bundled query lowers through L1:\n{stderr}"
    );
    String::from_utf8(output.stdout)
        .expect("ryi emits UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("the family emits JSON"))
        .collect()
}
