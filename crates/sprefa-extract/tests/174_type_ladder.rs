#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("type_ladder_outputs", |case| {
        crate::fixture_runner::commands(case, |_| serde_json::Value::Null)
    });
}
