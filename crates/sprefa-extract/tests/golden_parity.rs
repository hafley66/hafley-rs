//! Captured v5 facets and SCIP ratchets over fixture tables.
//! Existing oracles and floor/ceiling checks remain active in the evaluator.

#[test]
fn whole_output() {
    let output =
        crate::fixture_runner::evaluate("golden_parity", crate::golden_parity_support::evaluate);
    crate::fixture_runner::snapshot("golden_parity", output);
}
