#![cfg(feature = "cli")]

use std::path::{Path, PathBuf};
use std::process::Command;

fn files(root: &Path, dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read generated directory") {
        let path = entry.expect("generated entry").path();
        if path.is_dir() {
            files(root, &path, found);
        } else {
            found.push(path.strip_prefix(root).expect("generated relative path").to_path_buf());
        }
    }
}

#[test]
fn committed_generated_contract_matches_just_gen_cli() {
    if std::env::var_os("HAFLEY_TSP").is_none() {
        eprintln!("skipped generated contract check: HAFLEY_TSP is absent");
        return;
    }
    let fresh = tempfile::tempdir().expect("fresh generation directory");
    let output = Command::new("just")
        .arg("gen-cli")
        .arg(fresh.path())
        .output()
        .expect("run just gen-cli");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let committed = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/bin/ryi/gen");
    let mut expected = Vec::new();
    let mut actual = Vec::new();
    files(&committed, &committed, &mut expected);
    files(fresh.path(), fresh.path(), &mut actual);
    expected.sort();
    actual.sort();
    assert_eq!(actual, expected, "generated file roster");
    for path in expected {
        assert_eq!(
            std::fs::read(fresh.path().join(&path)).expect("fresh generated file"),
            std::fs::read(committed.join(&path)).expect("committed generated file"),
            "{}",
            path.display()
        );
    }
}
