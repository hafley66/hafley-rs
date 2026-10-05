#![cfg(feature = "cli")]
#[path = "support/0_dogfood_fixture.rs"]
mod support;
use support::Fixture;
use std::process::Command;

#[test]
fn rename_resolves_its_positional_path_once_from_the_request_directory() {
    let fixture = Fixture::new(&[
        ("pkg/Cargo.toml", "[package]\nname = 'rename-path'\nversion = '0.0.0'\nedition = '2021'\n[workspace]\n"),
        ("pkg/src/lib.rs", "pub fn old() {}\npub fn caller() { old(); }\n"),
    ]);
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(&fixture.root)
        .args(["rename", "pkg/src/lib.rs#old", "renamed", "--root"])
        .arg(fixture.root.join("pkg"))
        .args(["--state"]).arg(fixture._directory.path().join("state"))
        .arg("--commit").env("KACHE_DISABLED", "1").env("RUST_LOG", "off")
        .output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    insta::assert_snapshot!(fixture.read("pkg/src/lib.rs"), @r#"
    pub fn renamed() {}
    pub fn caller() { renamed(); }
    "#);
}
