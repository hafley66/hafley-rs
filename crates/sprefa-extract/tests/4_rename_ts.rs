#[test]
fn whole_output() {
    crate::fixture_runner::run("rename_ts", |case| {
        crate::fixture_runner::commands(case, crate::fixture_runner::editing_api)
    });
}
