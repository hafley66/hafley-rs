#[test]
fn whole_output() {
    crate::fixture_runner::run("scip_facts_cli", crate::scip_facts_support::evaluate);
}
