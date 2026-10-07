//! The tracing seam: shared warn-by-default telemetry and the summary table.
//! Nine scenarios collapse into one output; nondeterministic content (wall
//! micros, pid, timestamps) stays in code asserts in
//! `support/26_tracing.rs`, deterministic tables freeze. FAIL-FIRST receipt:
//! `--bench` once printed one `eprintln!` line per file whose shape no test
//! read, so the numbers were never comparable across runs.
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("tracing_cases", crate::tracing_support::evaluate);
}
