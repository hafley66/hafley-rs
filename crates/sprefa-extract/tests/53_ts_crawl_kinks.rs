//! The kinks the TypeScript 5.9 entrypoint crawl found
//! (`plans/extract-crawl-2026-08-29/ts5.REPORT.md` section 6), one command
//! per kink over the fixtures the crawl minted under
//! `tests/fixtures/ts5_findings/`: the synthetic `<module>` def and its node
//! row, member calls on unknown receivers (array pushes drop, class methods
//! and non-builtin members keep their legs, namespace/`this` receivers
//! resolve, `callee_path` travels), and functions named as values (reference
//! rows, `value_ref` edges, no rows for bare parameter names). Every old
//! assert is a claim row in `tests/fixtures/ts_crawl_kinks_cases/0_kinks.json`.
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("ts_crawl_kinks_cases", crate::ts_crawl_kinks_support::evaluate);
}
