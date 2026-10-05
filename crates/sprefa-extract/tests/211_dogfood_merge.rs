#![cfg(feature = "cli")]
#[path = "support/0_dogfood_fixture.rs"]
mod support;
use support::Fixture;

#[test]
fn one_cleave_list_stage_merges_a_test_module_and_deletes_its_source_and_declaration() {
    let mut results = Vec::new();
    for verify in [false, true] {
        let fixture = Fixture::from_dir("dogfood_merge");
        let mut args = vec!["cleave", "--list", "merge.tsv", "--commit"];
        if verify {
            args.extend(["--verify", "exit 1"]);
        }
        let output = fixture.run(&fixture.root, &args);
        assert_eq!(
            output.status.success(),
            !verify,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stages = stdout
            .lines()
            .filter(|line| line.starts_with("stage ") && line.ends_with(" committed"))
            .count();
        let source = if fixture.root.join("tests/199_v5_parity.rs").exists() {
            fixture.read("tests/199_v5_parity.rs")
        } else {
            "<deleted>\n".to_string()
        };
        results.push(format!(
            "verify={verify} stages={stages}\nsource:\n{source}all:\n{}",
            fixture.read("tests/all.rs")
        ));
    }
    insta::assert_snapshot!(results.join("\n"), @r#"
verify=false stages=1
source:
<deleted>
all:
#[test]
fn existing() {}

// Module emptied by the batch.
#[test]
fn alpha() {}

#[test]
fn beta() {}

verify=true stages=1
source:
// Module emptied by the batch.
#[test]
fn alpha() {}

#[test]
fn beta() {}
all:
#[path = "199_v5_parity.rs"]
mod v5_parity;

#[test]
fn existing() {}
"#);
}
