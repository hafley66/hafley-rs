#![cfg(feature = "cli")]
#[path = "support/0_dogfood_fixture.rs"]
mod support;
use support::Fixture;

#[test]
fn cargo_ownership_selects_the_source_workspace_or_names_the_searched_manifest() {
    let mut results = Vec::new();
    for (case, source, dest, success) in [
        (
            "dogfood_workspace_nested",
            "nested/src/source.rs",
            "nested/src/dest.rs",
            true,
        ),
        (
            "dogfood_workspace_unowned",
            "src/source.rs",
            "src/dest.rs",
            false,
        ),
    ] {
        let fixture = Fixture::from_dir(case);
        let output = fixture.run(
            &fixture.root,
            &["cleave", &format!("{source}#lifted"), dest, "--commit"],
        );
        assert_eq!(
            output.status.success(),
            success,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        results.push(if success {
            format!(
                "source:\n{}dest:\n{}",
                fixture.read(source),
                fixture.read(dest)
            )
        } else {
            String::from_utf8(output.stderr)
                .unwrap()
                .replace(fixture.root.to_str().unwrap(), "$ROOT")
        });
    }
    insta::assert_snapshot!(results.join("\n----\n"), @r#"
    source:
    pub fn stayed() {}
    dest:
    pub fn existing() {}

    pub fn lifted() {}

    ----
    cleave source src/source.rs is in no module of the Cargo workspace rust-analyzer loaded from $ROOT/Cargo.toml
    "#);
}
