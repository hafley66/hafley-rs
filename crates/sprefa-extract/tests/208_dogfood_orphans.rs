#![cfg(feature = "cli")]
#[path = "support/0_dogfood_fixture.rs"]
mod support;
use support::Fixture;

#[test]
fn grouped_orphans_are_removed_together_and_an_empty_group_disappears() {
    let mut results = Vec::new();
    for imports in [
        "use crate::helpers::{a, b, c, keep};",
        "use crate::helpers::{a, b};\nuse crate::helpers::{c, keep};",
        "use crate::helpers::a;\nuse crate::helpers::b;\nuse crate::helpers::c;\nuse crate::helpers::keep;",
    ] {
    for tail in ["pub fn stayed() { keep(); }\n", "pub fn stayed() {}\n"] {
        let source = format!("{imports}\n\npub fn lifted() {{ a(); b(); c(); keep(); }}\n{tail}");
        let fixture = Fixture::new(&[
            ("Cargo.toml", "[package]\nname = \"orphans\"\nversion = \"0.0.0\"\nedition = \"2021\"\n[workspace]\n"),
            ("src/lib.rs", "pub mod source;\npub mod dest;\npub mod helpers;\n"),
            ("src/source.rs", &source),
            ("src/dest.rs", "pub fn existing() {}\n"),
            ("src/helpers.rs", "pub fn a() {}\npub fn b() {}\npub fn c() {}\npub fn keep() {}\n"),
        ]);
        let output = fixture.run(&fixture.root, &["cleave", "src/source.rs#lifted", "src/dest.rs", "--commit"]);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        results.push(fixture.read("src/source.rs"));
    }
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
