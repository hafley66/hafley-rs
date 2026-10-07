//! The `ryi query` CLI: flat JSONL for plain and alternating patterns,
//! predicate filtering, exit-two rejects (unknown lang, invalid query, bad
//! digest), the staged-blob `--digest` door, and the md/md_inline/html
//! grammars. Every original assertion is a step in
//! `tests/fixtures/query_cli_cases/`; the JSONL streams freeze path-stripped.
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("query_cli_cases", |case| {
        crate::fixture_runner::commands(case, |_| serde_json::Value::Null)
    });
}
