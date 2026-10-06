#[test]
fn whole_output() {
    crate::fixture_runner::run("ingest", crate::ingest_support::evaluate);
}
