//! Lane D, kotlin: a member call whose receiver no phase-1 leg typed gets no
//! same_file, module_plane or corpus_unique answer, and lands in the drop
//! channel; the free call keeps its name-match leg.

use std::process::Command;

use serde_json::Value;
const SRC: &str = "tests/fixtures/kotlin_untyped_receiver";

fn run(names: &[&str]) -> Vec<Value> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--arms".to_string(),
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
        .filter(|(callee, _, _, _)| callee == "push")
        .map(|(_, file, origin, _)| (file, origin))
        .collect()
}

#[test]
fn receiver_typed_call_binds() {
    // The control: `d.push(1)` with `d: Store` binds through the corpus
    // owner table, origin receiver.
    let rows = push_edges(&["lib.kt", "use.kt"]);
    assert!(
        rows.iter()
            .any(|(file, origin)| file == "lib.kt" && origin == "receiver"),
        "{rows:?}"
    );
}

#[test]
fn untyped_receiver_member_call_declines() {
    // D.1: `w.push(2)` and `q.push(3)` ride fn returns the receiver plane
    // cannot type, so the corpus answers with NO edge at all for them: no
    // same_file, no module_plane, no corpus_unique. The control is the only
    // `push` edge the invocation mints.
    let rows = push_edges(&["lib.kt", "use.kt"]);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(
        rows.iter()
            .all(|(file, origin)| file == "lib.kt" && origin == "receiver"),
        "{rows:?}"
    );
    // The two untyped sites land in the drop channel with the K2 reason set.
    let drops = drops(&["lib.kt", "use.kt"]);
    let use_push_drops: Vec<&(String, String, String)> = drops
        .iter()
        .filter(|(file, detail, _)| file == "use.kt" && detail == "push")
        .collect();
    assert_eq!(use_push_drops.len(), 2, "{drops:?}");
    assert!(
        use_push_drops
            .iter()
            .all(|(_, _, reason)| ["inferred", "no_corpus_def", "ambiguous"]
                .contains(&reason.as_str())),
        "{drops:?}"
    );
}

#[test]
fn free_call_keeps_name_match() {
    // D.1: the free `sole(4)` keeps the corpus leg; only receiver-carrying
    // sites are cut off.
    let rows = edges(&["lib.kt", "use.kt"]);
    assert!(
        rows.iter()
            .any(|(callee, file, origin, _)| callee == "sole"
                && file == "use.kt"
                && (*origin == "module_plane" || *origin == "corpus_unique")),
        "{rows:?}"
    );
}
