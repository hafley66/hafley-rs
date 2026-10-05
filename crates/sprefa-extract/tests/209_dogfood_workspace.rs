#![cfg(feature = "cli")]
#[path = "support/0_dogfood_fixture.rs"]
mod support;
use support::Fixture;

#[test]
fn cleave_loads_the_sources_own_workspace_under_an_outer_root() {
    let fixture = Fixture::new(&[
        ("Cargo.toml", "[workspace]\nmembers = [\"outer\"]\nexclude = [\"nested\"]\nresolver = \"2\"\n"),
        ("outer/Cargo.toml", "[package]\nname = \"outer\"\nversion = \"0.0.0\"\nedition = \"2021\"\n"),
        ("outer/src/lib.rs", "pub fn outer() {}\n"),
        ("nested/Cargo.toml", "[package]\nname = \"nested\"\nversion = \"0.0.0\"\nedition = \"2021\"\n[workspace]\n"),
        ("nested/src/lib.rs", "pub mod source;\npub mod dest;\n"),
        ("nested/src/source.rs", "pub fn lifted() {}\npub fn stayed() {}\n"),
        ("nested/src/dest.rs", "pub fn existing() {}\n"),
    ]);
    let output = fixture.run(&fixture.root, &["cleave", "nested/src/source.rs#lifted", "nested/src/dest.rs", "--commit"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    insta::assert_snapshot!(format!("source:\n{}dest:\n{}", fixture.read("nested/src/source.rs"), fixture.read("nested/src/dest.rs")), @r#"
    source:
    pub fn stayed() {}
    dest:
    pub fn existing() {}

    pub fn lifted() {}
    "#);
}

#[test]
fn a_file_outside_the_module_tree_abstains_naming_the_searched_manifest() {
    let fixture = Fixture::new(&[
        ("Cargo.toml", "[package]\nname = 'unowned'\nversion = '0.0.0'\nedition = '2021'\n[workspace]\n"),
        ("src/lib.rs", "pub mod dest;\n"),
        ("src/source.rs", "pub fn lifted() {}\n"),
        ("src/dest.rs", "pub fn existing() {}\n"),
    ]);
    let output = fixture.run(&fixture.root, &["cleave", "src/source.rs#lifted", "src/dest.rs"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap().replace(fixture.root.to_str().unwrap(), "$ROOT");
    insta::assert_snapshot!(stderr, @r#"cleave source src/source.rs is in no module of the Cargo workspace rust-analyzer loaded from $ROOT/Cargo.toml"#);
}
