#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("scmpp_rows", |case| crate::fixture_runner::commands(case, |_| serde_json::Value::Null));
}
