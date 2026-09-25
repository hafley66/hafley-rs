//! The roster, not the verb, decides which languages `ryi graph` walks: the
//! three arms answer over a Kotlin corpus with no Kotlin named in `0_graph.rs`.
#![cfg(feature = "cli")]

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

const CORPUS: &str = "tests/fixtures/kotlin_module_resolve";

fn trace_path(tag: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "sprefa_graph_kotlin_{tag}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    directory.join("graph-kotlin.json")
}

fn rows(tag: &str, arm: &str, name: &str) -> Vec<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(["graph", arm, name, CORPUS])
        .env("HAFLEY_TRACE", trace_path(tag))
        .env("RUST_LOG", "off")
        .output()
        .expect("graph binary runs");
    assert!(
        output.status.success(),
        "graph failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn field<'a>(row: &'a Value, key: &str) -> &'a str {
    row.get(key).and_then(Value::as_str).unwrap()
}

#[test]
fn callers_answers_over_a_kotlin_corpus() {
    // `build(1)` and `makeWidget(2)`: one aliased import, one wildcard, both
    // landing on the same declaration, so the projection carries both sites.
    let rows = rows("callers", "--callers", "makeWidget");
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| {
        field(row, "from_path").ends_with("model/Widget.kt")
            && field(row, "to_name") == "main"
            && field(row, "grade") == "+"
    }));
}

#[test]
fn reach_answers_over_a_kotlin_corpus() {
    let rows = rows("from", "--from", "main");
    assert_eq!(rows.len(), 6);
    let depths: Vec<u64> = rows
        .iter()
        .map(|row| row.get("depth").and_then(Value::as_u64).unwrap())
        .collect();
    assert_eq!(depths, vec![1, 1, 1, 1, 1, 2]);
    let names: Vec<&str> = rows.iter().map(|row| field(row, "name")).collect();
    assert_eq!(
        names,
        vec!["appHelper", "lone", "spin", "shared", "makeWidget", "Widget"]
    );
}

#[test]
fn uses_answers_over_a_kotlin_corpus() {
    let rows = rows("uses", "--uses", "Widget");
    assert_eq!(rows.len(), 2);
    let kinds: Vec<&str> = rows.iter().map(|row| field(row, "kind")).collect();
    assert_eq!(kinds, vec!["field", "impl"]);
    assert!(rows
        .iter()
        .all(|row| field(row, "to_path").ends_with("app/Main.kt")));
}
