#![cfg(feature = "cli")]

#[test]
fn v5_call_definitions_include_the_end_line() {
    let _snapshots = super::v5_support::snapshots();
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/v5_parity/0_call.rs"
    );
    let output = super::v5_support::run(&["--kinds", "call", "--lines"], path);
    insta::assert_snapshot!(
        "v5_parity__call_lines__v5_call_definitions_include_the_end_line",
        String::from_utf8(output.stdout).unwrap()
    );
}
