//! SCIP family behavior over fixture command sequences.

#[test]
fn whole_output() {
    let output = crate::fixture_runner::evaluate(
        "scip_families_cli",
        crate::scip_families_support::evaluate,
    );
    crate::fixture_runner::snapshot("scip_families_cli", output);
}
