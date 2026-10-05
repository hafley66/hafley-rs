#![cfg(feature = "cli")]
#[path = "support/0_dogfood_fixture.rs"]
mod support;
use support::Fixture;

#[test]
fn grouped_orphans_are_removed_together_and_an_empty_group_disappears() {
    let mut results = Vec::new();
    for case in [
        "dogfood_orphans/0_0",
        "dogfood_orphans/0_1",
        "dogfood_orphans/1_0",
        "dogfood_orphans/1_1",
        "dogfood_orphans/2_0",
        "dogfood_orphans/2_1",
    ] {
        let fixture = Fixture::from_dir(case);
        let output = fixture.run(
            &fixture.root,
            &["cleave", "src/source.rs#lifted", "src/dest.rs", "--commit"],
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        results.push(fixture.read("src/source.rs"));
    }
    insta::assert_snapshot!(results.join("\n----\n"), @r#"
    use crate::helpers::keep;

    pub fn stayed() { keep(); }

    ----
    pub fn stayed() {}

    ----
    use crate::helpers::keep;

    pub fn stayed() { keep(); }

    ----
    pub fn stayed() {}

    ----
    use crate::helpers::keep;

    pub fn stayed() { keep(); }

    ----
    pub fn stayed() {}
    "#);
}
