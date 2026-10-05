#![cfg(feature = "cli")]
#[path = "support/0_dogfood_fixture.rs"]
mod support;
use std::process::Command;
use support::Fixture;

#[test]
fn rename_resolves_its_positional_path_once_from_the_request_directory() {
    let mut results = Vec::new();
    for case in ["dogfood_rename"] {
        let fixture = Fixture::from_dir(case);
        let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
            .current_dir(&fixture.root)
            .args(["rename", "pkg/src/lib.rs#old", "renamed", "--root"])
            .arg(fixture.root.join("pkg"))
            .args(["--state"])
            .arg(fixture._directory.path().join("state"))
            .arg("--commit")
            .env("KACHE_DISABLED", "1")
            .env("RUST_LOG", "off")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        results.push(fixture.read("pkg/src/lib.rs"));
    }
    insta::assert_snapshot!(results.join("\n"), @r#"
    pub fn renamed() {}
    pub fn caller() { renamed(); }
    "#);
}
