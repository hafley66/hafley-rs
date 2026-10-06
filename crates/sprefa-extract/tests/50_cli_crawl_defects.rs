#[test]
fn whole_output() {
    crate::fixture_runner::run("cli_crawl_defects", crate::cli_crawl_support::evaluate);
}
