#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("call_ladder_contract", |case| {
        crate::fixture_runner::commands(case, crate::rust_call_contract_support::capture)
    });
}
