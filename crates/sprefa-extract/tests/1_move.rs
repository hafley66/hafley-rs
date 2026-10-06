#[test]
fn whole_output() {
    crate::fixture_runner::run("move", |case| {
        crate::fixture_runner::commands(case, |_| serde_json::Value::Null)
    });
}
