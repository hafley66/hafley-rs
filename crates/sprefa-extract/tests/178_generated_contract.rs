#![cfg(feature = "cli")]

use std::path::Path;
use std::process::Command;

#[test]
fn committed_generated_contract_matches_pnpm_gen() {
    let schema = Path::new(env!("CARGO_MANIFEST_DIR")).join("schema/cli");
    if !schema.join("node_modules").is_dir() {
        eprintln!("skipped generated contract check: run pnpm install in schema/cli");
        return;
    }
    let output = Command::new("pnpm")
        .arg("gen")
        .current_dir(&schema)
        .output()
        .expect("run pnpm gen");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let generated = [
        "../ryi-proto/src/gen",
        "../ryi/src/gen",
        "src/bin/ryi/gen",
        "src/bin/ryi/ops.rs",
    ];
    let status = Command::new("git")
        .args(["status", "--porcelain", "--"])
        .args(generated)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run git status");
    // The worktree column: staged regeneration passes, drift from it fails.
    let drift: Vec<&str> = std::str::from_utf8(&status.stdout)
        .unwrap()
        .lines()
        .filter(|line| line.as_bytes()[1] != b' ')
        .collect();
    assert_eq!(drift, Vec::<&str>::new());
}
