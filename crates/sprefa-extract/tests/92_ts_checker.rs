#![cfg(feature = "ts-checker")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("ts_checker_outputs", |case| {
        let case = crate::fixture_runner::replace_strings(
            case,
            &[("$STOCK_TSGO", &crate::stock_tsgo::executable())],
        );
        crate::stock_tsgo::normalize(crate::fixture_runner::commands(&case, |_| unreachable!()))
    });
}
