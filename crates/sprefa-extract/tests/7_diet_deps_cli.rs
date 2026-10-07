//! The `--deps` contract: diet module resolution, graded on shape here and on
//! numbers by `tools/1_madge_oracle.sh diet`.
//!
//! THE GOLDEN PINS JSONL FIELD NAMES. The v6 host decodes by top-level key, and
//! `--deps` reuses the `file_edge` record `--scip-deps` produces, so the module
//! graph is ONE relation regardless of which resolver filled it. A rename on
//! either side is a breaking change and has to show up as a diff.
//!
//! Every resolution policy in `src/deps.rs` has a line in `fixtures/deps/app.ts`
//! and a claim in `support/22_diet_deps.rs`. The three policies that produce NO
//! edge are asserted as absences AND as `file_unresolved` rows. Sabotage
//! receipts: freezing `fold_edges`' key kind to `named` fails the golden at 6
//! rows to 9; narrowing `fold_unresolved` to `Policy::RelativeUnresolved` fails
//! the stop table at 1 row to 3; making `renamed` return `Some(imported)`
//! unconditionally fails on the `imported: null` specifier rows.
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("diet_deps_cases", crate::diet_deps_support::evaluate);
}
