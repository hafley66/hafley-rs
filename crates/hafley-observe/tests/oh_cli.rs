extern crate hafley_observe as oh;

#[allow(unused_imports)]
use oh::test;
use std::process::Command;

#[oh::test(time_ms = 1000, logs = 2)]
fn help_names_lab_create_surface() {
    let output = Command::new(env!("CARGO_BIN_EXE_oh"))
        .arg("--help")
        .output()
        .expect("run oh --help");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "oh lab new --title <slug> --manifest <Cargo.toml> [--root <labs-dir>]"
    );
}

#[oh::test(time_ms = 1000, logs = 2)]
fn lab_new_rejects_non_kebab_case_title() {
    let output = Command::new(env!("CARGO_BIN_EXE_oh"))
        .args(["lab", "new", "--title", "Dead_Files"])
        .output()
        .expect("run invalid oh lab new");
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("title must be lowercase kebab-case"));
}
