#[test]
fn whole_output() {
    crate::fixture_runner::run("cleave_ts_contract", |case| crate::fixture_runner::commands(case, |_| serde_json::Value::Null));
}
