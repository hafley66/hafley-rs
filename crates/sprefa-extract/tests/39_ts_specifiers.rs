//! The TS corpus walk (arc 1) and the module-specifier rows the move rewrites
//! by (arc 2). Expected values are hand-derived from the fixture text, never
//! copied from the extractor's output; every old assert runs as code over the
//! tables in `support/29_ts_specifiers.rs` before the snapshot freezes them.
//! SABOTAGE RECEIPTS: dropping the `visit_ts_import_equals_declaration` arm
//! leaves row 11 missing; taking `named.span` instead of `import.source.span`
//! makes row 1's slice read `thing` instead of `'./b.ts'`; dropping
//! `node_modules` from `SKIP_DIRS` breaks the corpus length claim.
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("ts_specifier_cases", crate::ts_specifiers_support::evaluate);
}
