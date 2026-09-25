//! `ryi graph --uses NAME`: the `uses` view over `resolved_type_edge`. One
//! type declared in one file, referenced from two others.
#![cfg(feature = "cli")]

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn trace_path(tag: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "sprefa_graph_uses_{tag}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    directory.join("graph-uses.json")
}

fn run(tag: &str, name: &str) -> (Vec<Value>, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(["graph", "--uses", name])
        .arg("tests/fixtures/graph_ts")
        .env("HAFLEY_TRACE", trace_path(tag))
        .env("RUST_LOG", "off")
        .output()
        .expect("graph binary runs");
    assert!(
        output.status.success(),
        "graph failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    (rows, String::from_utf8(output.stderr).unwrap())
}

#[test]
fn a_type_used_in_two_files_answers_with_two_rows() {
    let (rows, stderr) = run("widget", "Widget");
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows,
        serde_json::from_str::<Vec<Value>>(
            r#"[
            {"record":"graph_edge","from_path":"tests/fixtures/graph_ts/widget_reader.ts","from_name":"readWidget","to_path":"tests/fixtures/graph_ts/widget.ts","to_name":"Widget","kind":"param","grade":"+","from_line":3,"to_line":null},
            {"record":"graph_edge","from_path":"tests/fixtures/graph_ts/widget_writer.ts","from_name":"makeWidget","to_path":"tests/fixtures/graph_ts/widget.ts","to_name":"Widget","kind":"returns","grade":"+","from_line":3,"to_line":null}
            ]"#,
        )
        .unwrap()
    );
    assert!(stderr.contains("2 edges: 2 +, 0 ~, 0 -"), "{stderr}");
}

#[test]
fn a_type_nobody_names_answers_with_nothing() {
    let (rows, stderr) = run("absent", "NoSuchType");
    assert_eq!(rows.len(), 0);
    assert!(stderr.contains("0 edges: 0 +, 0 ~, 0 -"), "{stderr}");
}

#[test]
fn the_three_arms_are_mutually_exclusive() {
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(["graph", "--uses", "Widget", "--callers", "chainB"])
        .arg("tests/fixtures/graph_ts")
        .env("HAFLEY_TRACE", trace_path("group"))
        .env("RUST_LOG", "off")
        .output()
        .expect("graph binary runs");
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("cannot be used with"), "{stderr}");
}

#[test]
fn an_arm_is_required() {
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(["graph"])
        .arg("tests/fixtures/graph_ts")
        .env("HAFLEY_TRACE", trace_path("bare"))
        .env("RUST_LOG", "off")
        .output()
        .expect("graph binary runs");
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("--callers"), "{stderr}");
}
