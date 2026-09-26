//! Compare the type ladder's rendered agreement table with CodeQL when the CLI is available.
#![cfg(feature = "cli")]

use std::path::Path;
use std::process::Command;

#[test]
fn type_ladder_codeql_baseline() {
    if Command::new("codeql").arg("version").output().is_err() {
        eprintln!("skipping type_ladder_codeql_baseline: codeql is absent from PATH");
        return;
    }

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest.join("tests/fixtures/type_ladder");
    let script = manifest.join("scripts/ryi-vs-codeql.sh");
    let scratch = tempfile::tempdir().expect("scratch directory");
    let output = Command::new(script)
        .arg(fixture)
        .arg("rust")
        .env("RYI_BIN", env!("CARGO_BIN_EXE_ryi"))
        .env("RYI_CODEQL_OUT", scratch.path())
        .output()
        .expect("run baseline script");
    assert!(
        output.status.success(),
        "baseline script failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("UTF-8 baseline output");
    let table = stdout
        .lines()
        .take_while(|line| !line.starts_with("sample"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_eq!(table, include_str!("fixtures/codeql_baseline/type_ladder.txt"));
}
