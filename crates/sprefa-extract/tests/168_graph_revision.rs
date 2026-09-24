//! Revision graph questions read committed blobs and diff path answers.
#![cfg(feature = "cli")]

use std::process::Command;

use serde_json::Value;

#[test]
fn a_path_added_between_commits_is_reported_once() {
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().join("repo");
    let built = Command::new("sh")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg("tests/fixtures/diff/make_repo.sh")
        .arg(&root)
        .output()
        .unwrap();
    assert!(built.status.success());
    let shas: Vec<String> = String::from_utf8(built.stdout)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["graph", "--json", "--call-path", "beta", "--project-root"])
        .arg(&root)
        .args(["--at", &shas[0], "--compare", &shas[1], "."])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["record"], "graph_path_change");
    assert_eq!(rows[0]["change"], "added");
    assert_eq!(rows[0]["revision"], shas[1]);
    assert_eq!(rows[0]["from_name"], "beta");
    assert_eq!(rows[0]["to_name"], "alpha");
    assert_eq!(rows[0]["depth"], 1);
}
