#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("cli_http_parity", crate::http_support::evaluate);
}
