#[test]
fn whole_output() {
    crate::fixture_runner::run("mutation_battery", |case| {
        crate::fixture_runner::commands(case, crate::fixture_runner::resolved_rows)
    });
}
