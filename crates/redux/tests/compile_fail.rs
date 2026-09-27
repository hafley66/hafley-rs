//! Compile-fail and compile-pass coverage for the enum-state `machine!`.

#[test]
fn ui() {
    // kache rewrites source paths past trybuild's $DIR normalization. SAFETY:
    // this binary's only test sets these before trybuild spawns any thread.
    unsafe {
        std::env::set_var("CARGO_BUILD_RUSTC_WRAPPER", "");
        std::env::set_var("RUSTC_WRAPPER", "");
    }
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/*.rs");
    cases.pass("tests/ui-pass/*.rs");
}
