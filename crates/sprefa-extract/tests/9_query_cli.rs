//! The `ryi query` CLI: flat JSONL for plain and alternating patterns,
//! predicate filtering, exit-two rejects (unknown lang, invalid query, bad
//! digest), the staged-blob `--digest` door, and the md/md_inline/html
//! grammars. The old exact-string asserts run live in
//! `support/31_query_cli.rs`; the deterministic tables freeze path-stripped.
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("query_cli_cases", crate::query_cli_support::evaluate);
}
