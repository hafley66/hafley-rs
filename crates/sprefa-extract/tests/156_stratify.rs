//! `ryi stratify`: determinism, record shapes, entrypoint ranking, stem
//! preservation and collision omission, plus the move-TSV round trip through
//! `ryi move`. Every original assertion is a step in
//! `tests/fixtures/stratify_cases/`; the frozen snapshot carries the full
//! strata/locality/move tables.
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("stratify_cases", |case| {
        crate::fixture_runner::commands(case, |_| serde_json::Value::Null)
    });
}
