#[test]
fn whole_output() {
    crate::fixture_runner::run("scip_freshness", crate::freshness_support::evaluate);
}
