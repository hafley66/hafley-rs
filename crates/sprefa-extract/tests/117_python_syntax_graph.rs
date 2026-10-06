#[test]
fn whole_output() {
    crate::fixture_runner::run("python_syntax_graph", |case| {
        crate::fixture_runner::commands(case, |_| serde_json::Value::Null)
    });
}
