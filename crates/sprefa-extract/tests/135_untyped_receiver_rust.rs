//! Lane D, rust: a member call whose receiver no phase-1 leg typed gets no
//! same_file and no corpus_unique answer; free calls keep both.

use std::process::Command;

use serde_json::Value;
const SRC: &str = "tests/fixtures/rust_untyped_receiver";

fn run(names: &[&str]) -> Vec<Value> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--arms".to_string(),
        "call".to_string(),
    ];
    args.extend(names.iter().map(|name| format!("{SRC}/{name}")));
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(&args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{args:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a flat fact is JSON"))
        .collect()
}

fn text(row: &Value, key: &str) -> String {
    row[key].as_str().unwrap_or("").to_string()
}

fn edges(names: &[&str]) -> Vec<(String, String, String, String)> {
    let mut rows: Vec<(String, String, String, String)> = run(names)
        .iter()
        .filter(|row| row["record"] == "resolved_edge")
        .map(|row| {
            (
                text(row, "callee_name"),
                text(row, "callee_path")
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .to_string(),
                text(row, "resolution_origin"),
                text(row, "caller_name"),
            )
        })
        .collect();
    rows.sort();
    rows
}

fn drops(names: &[&str]) -> Vec<(String, String, String)> {
    let mut rows: Vec<(String, String, String)> = run(names)
        .iter()
        .filter(|row| row["record"] == "unresolved" && row["family"] == "call")
        .map(|row| {
            (
                text(row, "path")
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .to_string(),
                text(row, "detail"),
                text(row, "reason"),
            )
        })
        .collect();
    rows.sort();
    rows
}

fn push_edges(names: &[&str]) -> Vec<(String, String)> {
    edges(names)
        .into_iter()
        .filter(|(callee, _, _, _)| callee == "push")
        .map(|(_, file, origin, _)| (file, origin))
        .collect()
}





#[test]
fn free_call_keeps_name_match() {
    // D.1: the free `push(4)` keeps the same_file leg; the corpus-wide legs
    // stay reachable for free names.
    let rows = edges(&["lib.rs", "use.rs"]);
    assert!(
        rows.iter().any(|(callee, file, origin, _)| callee == "push"
            && file == "use.rs"
            && origin == "module_plane"),
        "{rows:?}"
    );
    assert!(
        rows.iter().any(|(callee, file, origin, _)| callee == "mk"
            && file == "use.rs"
            && origin == "module_plane"),
        "{rows:?}"
    );
}

#[path = "support/0_rust_names_call_contract.rs"]
mod names_contract;

#[test]
fn names_method_fixture_table() {
    for (case, actual, expected) in [
        ("untyped", names_contract::call_contract(&run(&["lib.rs", "use.rs"])), r#"drop lib.rs:102 push needs_types
drop use.rs:125 push needs_types
drop use.rs:49 push needs_types
drop use.rs:74 Vec::new no_corpus_def
drop use.rs:92 push needs_types
edge use.rs:113 f -> use.rs:181 mk name_resolve module_plane
edge use.rs:138 f -> use.rs:157 push name_resolve module_plane"#),
    ] {
        assert_eq!(actual, expected, "{case}");
    }
}
