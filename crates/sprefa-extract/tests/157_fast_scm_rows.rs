//! Fast's scm rows: wire shape, TypeSpec shape, and `ryi fast`'s own
//! stream agreeing with both.

#![cfg(feature = "cli")]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;
use sprefa_extract::{diet_scip, diet_scip_with_raw, scm_facts, FlatFact};

const CATALOG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/schema/generated/5_facts.json"
));

const KOTLIN_FILES: [&str; 2] = [
    "tests/fixtures/kotlin_receivers/lib.kt",
    "tests/fixtures/kotlin_receivers/use.kt",
];

/// The scm row vocabulary of `ryi fast`, tag by field set.
fn wire_shapes() -> BTreeMap<&'static str, Vec<&'static str>> {
    BTreeMap::from([
        ("symbol", vec!["kind", "path", "symbol"]),
        (
            "occurrence",
            vec![
                "decl_end",
                "decl_start",
                "end",
                "exported",
                "path",
                "role",
                "start",
                "symbol",
            ],
        ),
        ("local", vec!["end", "fn", "name", "path", "start"]),
        (
            "free_name",
            vec!["end", "name", "owner_end", "owner_start", "path", "start"],
        ),
    ])
}

fn samples() -> Vec<FlatFact> {
    vec![
        FlatFact::SymbolRow {
            symbol: "scm . . `a.kt`/run().".into(),
            path: "a.kt".into(),
            kind: "function".into(),
        },
        FlatFact::OccurrenceRow {
            symbol: "scm . . `a.kt`/run().".into(),
            path: "a.kt".into(),
            start: 1,
            end: 4,
            role: "def".into(),
            exported: true,
            decl_start: 0,
            decl_end: 20,
        },
        FlatFact::LocalRow {
            enclosing_fn: "run".into(),
            name: "x".into(),
            path: "a.kt".into(),
            start: 5,
            end: 6,
        },
        FlatFact::FreeNameRow {
            path: "a.kt".into(),
            owner_start: 0,
            owner_end: 20,
            name: "println".into(),
            start: 7,
            end: 14,
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
fn the_rows_carry_their_field_sets() {
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
fn fast_streams_the_rows_in_their_pinned_shape() {
    let shapes = wire_shapes();
    let mut seen = BTreeSet::new();
    for line in fast(&KOTLIN_FILES, 0).lines() {
        let value: Value = serde_json::from_str(line).expect("fast emits JSON");
        let record = value["record"].as_str().expect("record tag").to_string();
        let Some(want) = shapes.get(record.as_str()) else {
            continue;
        };
        assert_eq!(keys(&value), *want, "streamed shape: {record}");
        seen.insert(record);
    }
    assert_eq!(
        seen,
        shapes.keys().map(|k| (*k).to_string()).collect(),
        "fast streams every scm row over {KOTLIN_FILES:?}"
    );
}

#[test]
fn kotlin_fast_rows_match_the_file_query_for_identical_blobs_at_distinct_paths() {
    let root = std::env::temp_dir().join(format!("ryi-157-owned-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let paths: Vec<PathBuf> = ["0_copy.kt", "1_copy.kt"]
        .into_iter()
        .map(|name| root.join(name))
        .collect();
    let source = include_bytes!("fixtures/kotlin_receivers/lib.kt");
    for path in &paths {
        std::fs::write(path, source).unwrap();
    }
    let row_shapes = wire_shapes();
    let only_scm = |facts: Vec<FlatFact>| {
        facts
            .into_iter()
            .map(|fact| serde_json::to_value(fact).unwrap())
            .filter(|fact| row_shapes.contains_key(fact["record"].as_str().unwrap()))
            .collect::<Vec<_>>()
    };
    let expected = only_scm(scm_facts(&paths).unwrap());
    let actual = only_scm(diet_scip(&paths).unwrap());
    assert_eq!(actual, expected);
    let with_raw = diet_scip_with_raw(&paths, &mut |_| Ok::<_, ()>(())).unwrap();
    assert_eq!(only_scm(with_raw), expected);
    std::fs::remove_dir_all(root).unwrap();
}

/// A language with no bundled `.scm` is not a stop: fast still answers for it,
/// it just contributes none of the scm rows.
#[test]
fn a_language_with_no_query_contributes_no_scm_rows() {
    let shapes = wire_shapes();
    for line in fast(&["tests/fixtures/go_binding_legs/lib.go"], 2).lines() {
        let value: Value = serde_json::from_str(line).expect("fast emits JSON");
        let record = value["record"].as_str().expect("record tag");
        assert!(
            !shapes.contains_key(record),
            "a go file has no bundled scm query, so no {record} row: {line}"
        );
    }
}

fn trace_path(index: usize) -> PathBuf {
    std::env::temp_dir().join(format!("ryi-157-{index}-{}.json", std::process::id()))
}

fn fast(paths: &[&str], index: usize) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .arg("fast")
        .args(paths)
        .env("HAFLEY_TRACE", trace_path(index))
        .env("RUST_LOG", "sprefa_extract=debug")
        .output()
        .expect("ryi runs");
    assert!(
        output.status.success(),
        "ryi fast {paths:?} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("ryi emits UTF-8")
}
