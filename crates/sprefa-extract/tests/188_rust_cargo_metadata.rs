use serde_json::Value;
use std::path::Path;
use std::process::Command;

use sprefa_extract::{cargo_workspace_metadata, rust_cargo_targets};

const FIXTURE: &str = "tests/fixtures/cargo_metadata";

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .expect("ryii starts")
}

#[test]
fn cargo_targets_supply_custom_roots_editions_and_package_edges() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    let metadata = cargo_workspace_metadata(&fixture).expect("fixture metadata loads");
    let targets = rust_cargo_targets(&metadata);
    let lib = fixture.join("crates/app/src/lib_entry.rs");
    let bin = fixture.join("crates/app/src/bin/x.rs");
    let dep = fixture.join("crates/real-dep/src/lib.rs");
    assert_eq!(targets[&lib].package_edition, "2021");
    assert_eq!(targets[&lib].edition, "2021");
    assert_eq!(targets[&bin].package_edition, "2021");
    assert_eq!(targets[&bin].edition, "2021");
    assert_eq!(targets[&dep].package_edition, "2018");
    assert_eq!(targets[&dep].edition, "2018");
    assert!(targets
        .keys()
        .all(|path| !path.to_string_lossy().contains("excluded")));

    let rust_files = [
        "tests/fixtures/cargo_metadata/crates/app/src/lib_entry.rs",
        "tests/fixtures/cargo_metadata/crates/app/src/from_lib.rs",
        "tests/fixtures/cargo_metadata/crates/app/src/bin/x.rs",
        "tests/fixtures/cargo_metadata/crates/app/src/bin/from_bin.rs",
        "tests/fixtures/cargo_metadata/crates/real-dep/src/lib.rs",
    ];
    let mut deps_args = vec!["--deps", "--root", FIXTURE];
    deps_args.extend(rust_files);
    let output = run(&deps_args);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    for (source, target) in [
        ("crates/app/src/lib_entry.rs", "crates/app/src/from_lib.rs"),
        ("crates/app/src/bin/x.rs", "crates/app/src/bin/from_bin.rs"),
    ] {
        assert!(
            rows.iter().any(|row| row["record"] == "file_edge"
                && row["src_path"] == source
                && row["dst_path"] == target),
            "missing {source} -> {target}: {rows:?}"
        );
    }

    let manifests = [
        "tests/fixtures/cargo_metadata/Cargo.toml",
        "tests/fixtures/cargo_metadata/crates/app/Cargo.toml",
        "tests/fixtures/cargo_metadata/crates/real-dep/Cargo.toml",
        "tests/fixtures/cargo_metadata/crates/excluded/Cargo.toml",
    ];
    let mut package_args = vec!["--package-deps", "--root", FIXTURE];
    package_args.extend(manifests);
    let output = run(&package_args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let package_rows = String::from_utf8(output.stdout).unwrap();
    assert!(package_rows.contains(
        r#""src_manifest":"crates/app/Cargo.toml","dst_manifest":"crates/real-dep/Cargo.toml","kind":"normal""#
    ), "renamed path dependency missing: {package_rows}");
    assert!(
        !package_rows.contains("excluded"),
        "excluded package emitted: {package_rows}"
    );
}

#[test]
fn rust_inputs_without_cargo_report_the_named_metadata_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("PATH", "")
        .env_remove("CARGO")
        .args([
            "--deps",
            "--root",
            FIXTURE,
            "tests/fixtures/cargo_metadata/crates/app/src/lib_entry.rs",
        ])
        .output()
        .expect("ryii starts without cargo on PATH");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ryi: cargo metadata failed for")
            && stderr.contains("Rust crate roots need cargo"),
        "unexpected diagnostic: {stderr}"
    );
}
