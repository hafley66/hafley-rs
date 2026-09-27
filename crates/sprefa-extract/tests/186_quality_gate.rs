#![cfg(feature = "cli")]

use std::process::Command;

#[test]
fn query_rule_finds_direct_git_launch_in_fixture() {
    let temp = tempfile::tempdir().expect("temporary fixture directory");
    let source = temp.path().join("fixture.rs");
    std::fs::write(
        &source,
        "fn run() { let _ = std::process::Command::new(\"git\"); }\n",
    )
    .expect("write fixture");

    let query = concat!(env!("CARGO_MANIFEST_DIR"), "/gate/S038_command_new_git.scm");
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(["query", "--query"])
        .arg(std::fs::read_to_string(query).expect("read rule"))
        .arg(&source)
        .output()
        .expect("query binary runs");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("query output is UTF-8");
    assert!(stdout.contains("\"line\":1"), "expected hit, got {stdout}");
    assert!(
        stdout.contains("\"hit\":"),
        "expected captured call, got {stdout}"
    );
}

#[test]
fn repository_quality_gate_matches_its_allowlists() {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/quality-gate.sh");
    let output = Command::new(script)
        .arg(env!("CARGO_BIN_EXE_ryii"))
        .output()
        .expect("quality gate runs");
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
