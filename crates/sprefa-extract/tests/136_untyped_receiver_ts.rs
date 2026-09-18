//! Lane D, ts: a member call whose receiver no phase-1 leg traced (no row, or
//! an `Inferred` row) gets no name-match answer; free calls keep it.

use std::process::Command;

use serde_json::Value;
const SRC: &str = "tests/fixtures/ts_untyped_receiver";

fn run(names: &[&str]) -> Vec<Value> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--family".to_string(),
        "call".to_string(),
    ];
    args.extend(names.iter().map(|name| format!("{SRC}/{name}")));
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
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

fn edges(names: &[&str]) -> Vec<(String, String, String)> {
    let mut rows: Vec<(String, String, String)> = run(names)
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
                text(row, "path").rsplit('/').next().unwrap_or("").to_string(),
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
        .filter(|(callee, _, _)| callee == "push")
        .map(|(_, file, origin)| (file, origin))
        .collect()
}

#[test]
fn receiver_typed_call_binds() {
    // The control: `d.push(1)` with `d: Defs` binds through the declaring
    // class, origin receiver.
    let rows = push_edges(&["defs.ts", "use.ts"]);
    assert!(
        rows.iter()
            .any(|(file, origin)| file == "defs.ts" && origin == "receiver"),
        "{rows:?}"
    );
}

#[test]
fn untyped_receiver_member_call_drops_inferred() {
    // D.1: `w.push` and `q.push` carry `Inferred` rows; `mk().push` mints no
    // row but spells a member: all three drop `inferred`, no member edges.
    let rows = push_edges(&["defs.ts", "use.ts"]);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(
        rows.iter()
            .any(|(file, origin)| file == "defs.ts" && origin == "receiver"),
        "{rows:?}"
    );
    assert!(
        rows.iter()
            .any(|(file, origin)| file == "use.ts" && origin == "corpus_unique"),
        "{rows:?}"
    );
    let drops = drops(&["defs.ts", "use.ts"]);
    let use_push_drops: Vec<&(String, String, String)> = drops
        .iter()
        .filter(|(file, detail, _)| file == "use.ts" && detail.ends_with("push"))
        .collect();
    assert_eq!(use_push_drops.len(), 3, "{drops:?}");
    assert!(
        use_push_drops
            .iter()
            .all(|(_, _, reason)| reason == "inferred"),
        "{drops:?}"
    );
    // The no-row member spelling is the lane D receipt: its drop detail is the
    // written callee, `mk().push`, not the bare member name.
    assert!(
        use_push_drops
            .iter()
            .any(|(_, detail, _)| detail == "mk().push"),
        "{drops:?}"
    );
}

#[test]
fn free_call_keeps_name_match() {
    // D.1: free calls keep the corpus name match: `push(5)` and both `mk()`
    // sites bind to use.ts's own defs.
    let rows = edges(&["defs.ts", "use.ts"]);
    assert!(
        rows.iter()
            .any(|(callee, file, origin)| callee == "push"
                && file == "use.ts"
                && origin == "corpus_unique"),
        "{rows:?}"
    );
    assert_eq!(
        rows.iter()
            .filter(|(callee, file, origin)| callee == "mk"
                && file == "use.ts"
                && origin == "corpus_unique")
            .count(),
        2,
        "{rows:?}"
    );
}
