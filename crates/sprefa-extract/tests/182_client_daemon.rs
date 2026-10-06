#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("client_daemon", crate::daemon_support::evaluate);
}
