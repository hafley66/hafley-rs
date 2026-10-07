#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    let directory = if cfg!(feature = "ts-checker") {
        "graph_slow/with_ts"
    } else {
        "graph_slow/without_ts"
    };
    crate::fixture_runner::run(directory, |case| {
        crate::fixture_runner::commands(case, |_| serde_json::Value::Null)
    });
}
