//! The three `scip_scm` rows: wire shape, TypeSpec shape, and the binary's
//! own stream agreeing with both.

#![cfg(feature = "cli")]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use sprefa_extract::FlatFact;

const CATALOG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/schema/generated/5_facts.json"
));

/// The pass-1 row vocabulary, tag by field set.
fn wire_shapes() -> BTreeMap<&'static str, Vec<&'static str>> {
    BTreeMap::from([
        ("scip_scm_symbol", vec!["kind", "path", "symbol"]),
        (
            "scip_scm_occurrence",
            vec!["end", "path", "role", "start", "symbol"],
        ),
        (
            "scip_scm_local",
            vec!["end", "fn", "name", "path", "start"],
        ),
    ])
}

fn samples() -> Vec<FlatFact> {
    vec![
        FlatFact::ScipScmSymbolRow {
            symbol: "scm . . `a.kt`/run().".into(),
            path: "a.kt".into(),
            kind: "function".into(),
        },
        FlatFact::ScipScmOccurrenceRow {
            symbol: "scm . . `a.kt`/run().".into(),
            path: "a.kt".into(),
            start: 1,
            end: 4,
            role: "def".into(),
        },
        FlatFact::ScipScmLocalRow {
            enclosing_fn: "run".into(),
            name: "x".into(),
            path: "a.kt".into(),
            start: 5,
            end: 6,
        },
    ]
}

fn keys(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .expect("a flat fact serializes to an object")
        .keys()
        .filter(|key| *key != "record")
        .cloned()
        .collect();
    keys.sort();
    keys
}

#[test]
fn the_three_rows_carry_the_pass_one_field_sets() {
    let shapes = wire_shapes();
    for fact in samples() {
        let value = serde_json::to_value(&fact).expect("a flat fact is serializable");
        let record = value["record"].as_str().expect("record tag").to_string();
        let want = shapes
            .get(record.as_str())
            .unwrap_or_else(|| panic!("unexpected record: {record}"));
        assert_eq!(keys(&value), *want, "wire shape: {record}");
        let back: FlatFact = serde_json::from_value(value).expect("round trip");
        assert_eq!(
            serde_json::to_string(&back).unwrap(),
            serde_json::to_string(&fact).unwrap(),
            "round trip: {record}"
        );
    }
}

#[test]
fn every_row_has_a_typespec_table_with_the_same_columns() {
    let catalog: Value = serde_json::from_str(CATALOG).expect("the generated catalog parses");
    let tables = catalog.as_array().expect("the catalog is an array");
    for (record, want) in wire_shapes() {
        let table = tables
            .iter()
            .find(|table| table["record"] == record)
            .unwrap_or_else(|| panic!("no TypeSpec table for {record}: run node schema/2_gen.mjs"));
        let columns: BTreeSet<String> = table["columns"]
            .as_array()
            .expect("columns")
            .iter()
            .map(|column| column["path"][0].as_str().expect("column path").to_string())
            .filter(|path| path != "record" && !path.starts_with('_'))
            .collect();
        assert_eq!(
            columns,
            want.iter().map(|f| (*f).to_string()).collect(),
            "TypeSpec shape: {record}"
        );
    }
}

#[test]
fn the_family_streams_only_pass_one_rows() {
    let fixture = Path::new("tests/fixtures/kotlin_receivers");
    let output = ryi(&["--family", "scip_scm", &fixture.to_string_lossy()], 0);
    let shapes = wire_shapes();
    for line in output.lines() {
        let value: Value = serde_json::from_str(line).expect("the family emits JSON");
        let record = value["record"].as_str().expect("record tag").to_string();
        let want = shapes
            .get(record.as_str())
            .unwrap_or_else(|| panic!("the family emitted a foreign record: {record}"));
        assert_eq!(keys(&value), *want, "streamed shape: {record}");
    }
}

#[test]
fn the_mode_refuses_a_per_file_mask_beside_it() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ryi"));
    command
        .args(["--family", "scip_scm,cst", "tests/fixtures/kotlin_receivers"])
        .env("HAFLEY_TRACE", trace_path(1))
        .env("RUST_LOG", "sprefa_extract=debug");
    let output = command.output().expect("ryi runs");
    assert!(!output.status.success(), "a mode and a mask cannot combine");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("whole-project mode"),
        "the refusal names the reason: {stderr}"
    );
}

fn trace_path(index: usize) -> PathBuf {
    std::env::temp_dir().join(format!(
        "ryi-157-{index}-{}.json",
        std::process::id()
    ))
}

fn ryi(args: &[&str], index: usize) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ryi"));
    command
        .args(args)
        .env("HAFLEY_TRACE", trace_path(index))
        .env("RUST_LOG", "sprefa_extract=debug");
    let output = command.output().expect("ryi runs");
    assert!(
        output.status.success(),
        "ryi {args:?} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("ryi emits UTF-8")
}
