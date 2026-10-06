//! SCIP family behavior over fixture command sequences.

#[test]
fn whole_output() {
    crate::fixture_runner::run(
        "scip_families_cli",
        crate::scip_families_support::evaluate,
    );
}
