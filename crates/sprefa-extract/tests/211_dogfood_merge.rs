#![cfg(feature = "cli")]
#[path = "support/0_dogfood_fixture.rs"]
mod support;
use support::Fixture;

#[test]
fn one_cleave_list_stage_merges_a_test_module_and_deletes_its_source_and_declaration() {
    let fixture = Fixture::from_dir("dogfood_merge");
    let output = fixture.run(&fixture.root, &["cleave", "--list", "merge.tsv", "--commit"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stages = stdout.lines().filter(|line| line.starts_with("stage ") && line.ends_with(" committed")).count();
    insta::assert_snapshot!(format!("source_exists: {}\nstages: {stages}\nall:\n{}", fixture.root.join("tests/199_v5_parity.rs").exists(), fixture.read("tests/all.rs")), @r#"
    source_exists: false
    stages: 1
    all:
    #[test]
    fn existing() {}

    // Module emptied by the batch.
    #[test]
    fn alpha() {}

    #[test]
    fn beta() {}
    "#);
}
