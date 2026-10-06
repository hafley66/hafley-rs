//! Captured v5 facets and SCIP ratchets over fixture tables.
//! Existing oracles and floor/ceiling checks remain active in the evaluator.

#[test]
fn whole_output() {
    crate::fixture_runner::run("golden_parity", crate::golden_parity_support::evaluate);
}
