//! Byte parity between the Rust and `.scm` Kotlin CallF projectors.

use std::path::{Path, PathBuf};
use std::process::Command;

const FIXTURES: [&str; 2] = [
    "tests/fixtures/kotlin_receivers",
    "tests/fixtures/kotlin_module_resolve",
];

#[test]
fn kotlin_scm_call_rows_match_rust() {
    let mut differences = Vec::new();
    for (index, path) in fixture_files().into_iter().enumerate() {
        let rust = call_rows(&path, false, index);
        let scm = call_rows(&path, true, index);
        if rust != scm {
            let rust_only = rust
                .iter()
                .filter(|row| !scm.contains(row))
                .cloned()
                .collect::<Vec<_>>();
            let scm_only = scm
                .iter()
                .filter(|row| !rust.contains(row))
                .cloned()
                .collect::<Vec<_>>();
            differences.push(format!(
                "{}\n  rust only: {rust_only:#?}\n  scm only: {scm_only:#?}",
                path.display()
            ));
        }
    }
    assert!(
        differences.is_empty(),
        "Kotlin CallF rows differ:\n{}",
        differences.join("\n")
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

fn call_rows(path: &Path, scm: bool, index: usize) -> Vec<String> {
    let mode = if scm { "scm" } else { "rust" };
    let trace = format!("{}/traces/phase3-{mode}-{index}.json", env!("CARGO_MANIFEST_DIR"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_ryi"));
    command
        .args(["--family", "call"])
        .arg(path)
        .env("HAFLEY_TRACE", trace)
        .env("RUST_LOG", "sprefa_extract=debug");
    if scm {
        command.env("RYI_FAST_SCM", "1");
    } else {
        command.env_remove("RYI_FAST_SCM");
    }
    let output = command.output().expect("ryi runs");
    assert!(
        output.status.success(),
        "ryi {mode} {} failed:\n{}",
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
