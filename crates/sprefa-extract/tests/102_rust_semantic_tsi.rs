#![cfg(feature = "rust-checker")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("rust_semantic_tsi", |case| {
        crate::fixture_runner::commands(case, |_| serde_json::Value::Null)
    });
}
