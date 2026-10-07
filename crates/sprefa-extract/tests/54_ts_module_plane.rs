#[test]
fn whole_output() {
    crate::fixture_runner::run("ts_module_plane", |case| {
        crate::fixture_runner::commands(case, |_| serde_json::Value::Null)
    });
}
