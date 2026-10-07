#![cfg(feature = "cli")]

use std::process::Command;

#[test]
fn server_exposes_direct_cli_mode() {
    let binary = env!("CARGO_BIN_EXE_ryii");
    let direct = Command::new(binary)
        .args(["fast", "--help"])
        .output()
        .expect("server cli help");
    assert!(direct.status.success());
    assert!(String::from_utf8_lossy(&direct.stdout).contains("Usage: ryi fast"));
}
