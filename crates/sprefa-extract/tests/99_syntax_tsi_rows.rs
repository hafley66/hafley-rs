#[test]
fn whole_output() {
    crate::fixture_runner::run("syntax_tsi_rows", crate::tsi_rows_support::evaluate);
}
