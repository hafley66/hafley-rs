//! Golden: the Kotlin CallF rows `ryi fast` reads from `queries/kotlin/call.scm`.

use std::path::{Path, PathBuf};
use std::process::Command;

const FIXTURES: [&str; 2] = [
    "tests/fixtures/kotlin_receivers",
    "tests/fixtures/kotlin_module_resolve",
];

const GOLDEN: &str = include_str!("goldens/150_kotlin_call_scm.jsonl");

#[test]
fn the_kotlin_call_rows_answer_the_golden() {
    let mut lines = Vec::new();
    for (index, path) in fixture_files().into_iter().enumerate() {
        let name = path.to_string_lossy().to_string();
        for row in call_rows(&path, index) {
            lines.push(format!("{name}\t{row}"));
        }
    }
    assert_eq!(
        lines.join("\n"),
        GOLDEN.trim_end_matches('\n'),
        "Kotlin CallF rows from the scm path"
    );
}

fn fixture_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for fixture in FIXTURES {
        collect_files(Path::new(fixture), &mut files);
    }
    files.sort();
    files
}

fn collect_files(path: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).expect("fixture directory") {
        let path = entry.expect("fixture entry").path();
        if path.is_dir() {
            collect_files(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("kt") {
            files.push(path);
        }
    }
}

fn call_rows(path: &Path, index: usize) -> Vec<String> {
    let trace =
        std::env::temp_dir().join(format!("ryi-150-{index}-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(["--kinds", "call"])
        .arg(path)
        .env("HAFLEY_TRACE", trace)
        .env("RUST_LOG", "sprefa_extract=debug")
        .output()
        .expect("ryi runs");
    assert!(
        output.status.success(),
        "ryi --family call {} failed:\n{}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let mut rows = String::from_utf8(output.stdout)
        .expect("ryi emits UTF-8")
        .lines()
        .map(str::to_string)
        .collect::<Vec<_>>();
    rows.sort();
    rows
}
