#[test]
fn whole_output() {
    crate::fixture_runner::run("move_cross_crate", |case| {
        crate::fixture_runner::commands(case, |_| serde_json::Value::Null)
    });
}
