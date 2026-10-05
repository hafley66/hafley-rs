#![cfg(feature = "cli")]
#[path = "support/0_dogfood_fixture.rs"]
mod support;
use support::Fixture;

#[test]
fn public_reexport_routes_survive_a_cleave_into_a_private_module() {
    let mut results = Vec::new();
    for case in ["dogfood_reexport", "dogfood_private_destination"] {
        let fixture = Fixture::from_dir(case);
        let output = fixture.run(&fixture.root, &["cleave", "src/0_source.rs#covering_def", "src/1_flow.rs", "--commit"]);
        if case == "dogfood_private_destination" {
            assert!(!output.status.success());
            results.push(format!("{}caller:\n{}", String::from_utf8_lossy(&output.stderr), fixture.read("src/caller.rs")));
            continue;
        }
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        results.push(format!("types:\n{}caller:\n{}", fixture.read("src/types.rs"), fixture.read("src/caller.rs")));
    }
    insta::assert_snapshot!(results.join("\n----\n"), @r#"
    types:
    #[path = "0_source.rs"]
    mod source;
    #[path = "1_flow.rs"]
    mod flow;
    pub use crate::types::flow::covering_def;
    caller:
    use crate::types::covering_def;
    pub fn caller() { covering_def(); crate::types::covering_def(); }

    ----
    cleave path src/1_flow.rs#covering_def is not visible from src/caller.rs
    caller:
    use crate::types::source::covering_def;
    pub fn caller() { covering_def(); crate::types::source::covering_def(); }
    "#);
}
