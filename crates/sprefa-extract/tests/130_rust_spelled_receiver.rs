//! Lane C, rust: a SPELLED receiver records Named(T) and binds through the
//! corpus impl table; fixtures under tests/fixtures/rust_spelled_receiver/src/.

use std::process::Command;

use serde_json::Value;
const SRC: &str = "tests/fixtures/rust_spelled_receiver/src";

fn run(names: &[&str]) -> Vec<Value> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--family".to_string(),
        "call".to_string(),
    ];
    args.extend(names.iter().map(|name| format!("{SRC}/{name}")));
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
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
                text(row, "caller_name"),
                text(row, "callee_name"),
                text(row, "callee_path")
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .trim_end_matches(".rs")
                    .to_string(),
                text(row, "resolution_origin"),
            )
        })
        .collect();
    rows.sort();
    rows
}

fn has_origin(
    rows: &[(String, String, String, String)],
    caller: &str,
    callee: &str,
    file: &str,
    origin: &str,
) -> bool {
    rows.iter()
        .any(|(c, n, f, o)| c == caller && n == callee && f == file && o == origin)
}

#[test]
fn spelled_unit_struct_receiver_binds() {
    // C.1/C.2: `CstProjector.project()` records a Named receiver and binds to
    // the corpus `impl Project for CstProjector` in proj.rs.
    let rows = edges(&["proj.rs"]);
    assert!(
        has_origin(&rows, "spelled", "project", "proj", "receiver"),
        "{rows:?}"
    );
}
#[test]
fn field_typed_receiver_binds() {
    // C.5 field leg: `b.inner.run()` where `struct Box { inner: Widget }`.
    let rows = edges(&["proj.rs"]);
    assert!(
        has_origin(&rows, "field_leg", "run", "proj", "receiver"),
        "{rows:?}"
    );
}

#[test]
fn constructor_return_receiver_binds() {
    // C.5 constructor-return leg: `let w = Widget::new(); w.run()`.
    let rows = edges(&["proj.rs"]);
    assert!(
        has_origin(&rows, "ctor_leg", "run", "proj", "receiver"),
        "{rows:?}"
    );
}

#[test]
fn trait_bound_generic_receiver_binds() {
    // C.5 trait-bound generic leg: `fn f<P: Proj>(p: P) { p.run() }` binds to
    // the trait's own fn def in proj.rs.
    let rows = edges(&["proj.rs"]);
    assert!(
        has_origin(&rows, "trait_bound_leg", "run", "proj", "receiver"),
        "{rows:?}"
    );
}

fn drops(names: &[&str]) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = run(names)
        .iter()
        .filter(|row| row["record"] == "unresolved" && row["family"] == "call")
        .map(|row| (text(row, "detail"), text(row, "reason")))
        .collect();
    rows.sort();
    rows
}

#[test]
fn shadowed_call_does_not_bind_free_fn() {
    // C.6: `fn run(project: impl Fn()) { project() }` names the scope param,
    // never the free `fn project` in free.rs: zero edges, drop `inferred`.
    let rows = edges(&["shadow.rs", "free.rs"]);
    assert!(
        !rows.iter().any(|(_, callee, file, _)| callee == "project" && file == "free"),
        "{rows:?}"
    );
    let drops = drops(&["shadow.rs", "free.rs"]);
    assert!(
        drops
            .iter()
            .any(|(detail, reason)| detail == "project" && reason == "inferred"),
        "{drops:?}"
    );
}
