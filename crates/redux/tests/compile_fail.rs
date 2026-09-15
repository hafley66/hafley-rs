//! Compile-fail and compile-pass coverage for the enum-state `machine!`.

#[test]
fn ui() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/*.rs");
    cases.pass("tests/ui-pass/*.rs");
}
