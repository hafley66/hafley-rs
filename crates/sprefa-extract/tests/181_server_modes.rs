#![cfg(feature = "cli")]

use std::process::Command;

#[test]
fn server_exposes_build_stamp_and_direct_cli_mode() {
    let binary = env!("CARGO_BIN_EXE_ryi-server");
    let stamp = Command::new(binary).arg("--stamp").output().expect("server stamp");
    assert!(stamp.status.success());
    let stamp = String::from_utf8(stamp.stdout).expect("UTF-8 stamp");
    assert!(stamp.contains(env!("SPREFA_BUILD_GIT_HASH")));
    assert!(stamp.contains(env!("SPREFA_BUILD_DATETIME")));

    let direct = Command::new(binary).args(["fast", "--help"]).output().expect("server cli help");
    assert!(direct.status.success());
    assert!(String::from_utf8_lossy(&direct.stdout).contains("Usage: ryi fast"));
}
