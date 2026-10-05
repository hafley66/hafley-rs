#![cfg(feature = "cli")]
#[path = "support/0_dogfood_fixture.rs"]
mod support;
use support::Fixture;

#[test]
fn cleave_list_emits_one_json_plan_line_per_row() {
    let mut results = Vec::new();
    for mode in ["dry", "commit"] {
        let fixture = Fixture::from_dir("dogfood_merge");
        let mut args = vec!["cleave", "--list", "merge.tsv", "--json"];
        if mode == "commit" { args.push("--commit"); }
        let output = fixture.run(&fixture.root, &args);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let stdout = String::from_utf8(output.stdout).unwrap();
        for line in stdout.lines() { let _: serde_json::Value = serde_json::from_str(line).unwrap(); }
        results.push(format!("{mode}:\n{stdout}"));
    }
    insta::assert_snapshot!(results.join("\n"));
}
