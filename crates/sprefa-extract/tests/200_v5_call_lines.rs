#![cfg(feature = "cli")]

#[test]
fn v5_call_definitions_include_the_end_line() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/v5_parity/0_call.rs"
    );
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(["--kinds", "call", "--lines", path])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    insta::assert_snapshot!(String::from_utf8(output.stdout).unwrap());
}
