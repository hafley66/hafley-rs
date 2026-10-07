//! The ts syntax tier's best-guess type graph over
//! `tests/fixtures/tsi/probe_graph.ts`, the twin of `106_rust_syntax_graph`:
//! one witnessed `--kinds type` run whose complete fact stream — applications,
//! anonymous products/callables, typed bindings, primitive classes, names and
//! origins — freezes in the snapshot; every original assertion reads those
//! relation rows.
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("ts_syntax_graph", |case| {
        crate::fixture_runner::commands(case, |_| serde_json::Value::Null)
    });
}
