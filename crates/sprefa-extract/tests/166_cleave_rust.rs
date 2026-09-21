//! `ryi cleave` on the Rust arm, over `tests/fixtures/cleave_rust`: the same
//! three-file step trace the TypeScript fixture states, plus the drag variant,
//! each proved by a `cargo check` on the tree the run leaves behind.

#![cfg(feature = "cli")]

use std::path::{Path, PathBuf};
use std::process::Command;

struct Fixture {
    root: PathBuf,
    state: PathBuf,
    target: PathBuf,
    trace: PathBuf,
}

fn fixture(variant: &str, label: &str) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "ryi_cleave_rust_{variant}_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = base.join("repo");
    let state = base.join("state");
    std::fs::create_dir_all(&state).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cleave_rust")
        .join(variant);
    copy_tree(&source, &root);
    git(&root, &["init", "-q", "."]);
    Fixture {
        root: root.canonicalize().unwrap(),
        state,
        target: base.join("target"),
        trace: base.join("cleave.json"),
    }
}

fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read fixture dir") {
        let entry = entry.expect("fixture entry");
        let to = target.join(entry.file_name());
        match entry.file_type().expect("file type").is_dir() {
            true => copy_tree(&entry.path(), &to),
            false => {
                std::fs::copy(entry.path(), &to).expect("copy fixture file");
            }
        }
    }
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn cleave(fixture: &Fixture, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .arg("cleave")
        .args(args)
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .current_dir(&fixture.root)
        .env("HAFLEY_TRACE", &fixture.trace)
        .env("RUST_LOG", "sprefa_extract=debug")
        .output()
        .expect("cleave binary runs");
    let stdout = String::from_utf8(output.stdout).expect("cleave stdout is UTF-8");
    assert_eq!(
        output.status.code(),
        Some(0),
        "cleave {args:?} exited {:?}:\n{stdout}\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

/// The tree the run left behind, compiled. A cleave that type checks is the
/// only receipt that says the imports it wrote are the imports it needed.
fn cargo_check(fixture: &Fixture) -> String {
    let output = Command::new(env!("CARGO"))
        .args(["check", "--quiet"])
        .current_dir(&fixture.root)
        .env("CARGO_TARGET_DIR", &fixture.target)
        .output()
        .expect("cargo check runs");
    let report = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "cargo check on the cleaved tree failed:\n{report}"
    );
    report
}

fn read(fixture: &Fixture, rel: &str) -> String {
    std::fs::read_to_string(fixture.root.join(rel)).expect("read fixture file")
}

fn use_lines(fixture: &Fixture, rel: &str) -> usize {
    read(fixture, rel)
        .lines()
        .filter(|line| line.starts_with("use "))
        .count()
}

fn plan_of(stdout: &str) -> serde_json::Value {
    let line = stdout
        .lines()
        .find(|line| line.starts_with('{'))
        .expect("a cleave_plan json line");
    serde_json::from_str(line).expect("cleave_plan parses")
}

fn names(plan: &serde_json::Value, key: &str) -> Vec<String> {
    plan[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} is an array"))
        .iter()
        .map(|row| row["name"].as_str().expect("a name").to_string())
        .collect()
}

fn field(plan: &serde_json::Value, key: &str, column: &str) -> Vec<String> {
    plan[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} is an array"))
        .iter()
        .map(|row| row[column].as_str().expect("a column").to_string())
        .collect()
}

#[test]
fn the_plan_partitions_the_same_three_file_trace_the_ts_fixture_states() {
    let fixture = fixture("basic", "plan");
    let plan = plan_of(&cleave(
        &fixture,
        &["src/util.rs#load_config", "src/config.rs", "--json"],
    ));

    assert_eq!(plan["record"], "cleave_plan");
    assert_eq!(plan["src"], "src/util.rs");
    assert_eq!(plan["dest"], "src/config.rs");
    assert_eq!(plan["item"], "load_config");
    assert_eq!(
        names(&plan, "travelling"),
        ["read_to_string", "Path", "log_line"]
    );
    assert_eq!(
        names(&plan, "orphans"),
        ["read_to_string", "Path", "log_line"]
    );
    // `Path` already sits in config.rs, so it travels as `carried` and lands
    // nowhere; `crate::log` resolves to a corpus file, so it is `relative`.
    assert_eq!(
        field(&plan, "travelling", "kind"),
        ["package", "carried", "relative"]
    );
    assert_eq!(
        field(&plan, "travelling", "dest_module"),
        ["std::fs::read_to_string", "std::path::Path", "crate::log"]
    );
    assert_eq!(
        plan["callers"].as_array().unwrap(),
        &vec![serde_json::json!("src/app.rs")]
    );
    assert_eq!(plan["dragged"].as_array().unwrap().len(), 0);
    assert_eq!(plan["drag_iterations"], 1);
    assert_eq!(plan["unresolved"].as_array().unwrap().len(), 0);
}

#[test]
fn a_commit_moves_the_item_its_uses_and_the_caller_and_still_compiles() {
    let fixture = fixture("basic", "commit");
    assert_eq!(use_lines(&fixture, "src/util.rs"), 3);
    assert_eq!(use_lines(&fixture, "src/config.rs"), 1);
    assert_eq!(use_lines(&fixture, "src/app.rs"), 1);

    cleave(
        &fixture,
        &["src/util.rs#load_config", "src/config.rs", "--commit"],
    );

    assert_eq!(use_lines(&fixture, "src/util.rs"), 0, "3 use lines left");
    assert_eq!(use_lines(&fixture, "src/config.rs"), 3, "2 use lines came");
    assert_eq!(use_lines(&fixture, "src/app.rs"), 2, "the caller split");
    assert_eq!(
        read(&fixture, "src/util.rs"),
        "pub fn slug(raw: &str) -> String {\n    raw.to_lowercase()\n}\n"
    );
    assert_eq!(
        read(&fixture, "src/app.rs"),
        "use crate::util::slug;\n\
         use crate::config::load_config;\n\
         \n\
         pub fn boot(dir: &str) -> String {\n    slug(&load_config(dir))\n}\n",
        "a two-name use tree loses its braces when one name leaves"
    );
    cargo_check(&fixture);
}

#[test]
fn a_missing_destination_is_created_with_every_use_line() {
    let fixture = fixture("basic", "create");
    cleave(
        &fixture,
        &["src/util.rs#load_config", "src/loader.rs", "--commit"],
    );
    std::fs::write(
        fixture.root.join("src/lib.rs"),
        "pub mod app;\npub mod config;\npub mod loader;\npub mod log;\npub mod util;\n",
    )
    .unwrap();
    assert_eq!(use_lines(&fixture, "src/loader.rs"), 3);
    assert!(read(&fixture, "src/app.rs").contains("use crate::loader::load_config;"));
    cargo_check(&fixture);
}

#[test]
fn drag_moves_the_sole_user_helper_and_exports_the_shared_one() {
    let fixture = fixture("drag", "split");
    let plan = plan_of(&cleave(
        &fixture,
        &[
            "src/util.rs#load_config",
            "src/config.rs",
            "--drag",
            "--json",
        ],
    ));
    assert_eq!(names(&plan, "dragged"), ["pad", "slug"]);
    assert_eq!(field(&plan, "dragged", "action"), ["exported", "moved"]);
    assert_eq!(plan["drag_iterations"], 1);

    cleave(
        &fixture,
        &[
            "src/util.rs#load_config",
            "src/config.rs",
            "--drag",
            "--commit",
        ],
    );
    let util = read(&fixture, "src/util.rs");
    assert!(util.contains("pub fn pad("), "pad widened to pub");
    assert!(!util.contains("fn slug("), "slug left");
    let config = read(&fixture, "src/config.rs");
    assert_eq!(config.matches("fn slug(").count(), 1, "slug landed once");
    assert_eq!(
        config.matches("use crate::util::pad;").count(),
        1,
        "the destination imports the shared helper from the source"
    );
    assert_eq!(config.matches("fn pad(").count(), 0, "never copied");
    cargo_check(&fixture);
}

#[test]
fn a_failed_verify_rolls_the_rust_tree_back() {
    let fixture = fixture("basic", "verify");
    let before = read(&fixture, "src/util.rs");
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args([
            "cleave",
            "src/util.rs#load_config",
            "src/config.rs",
            "--commit",
            "--verify",
            "exit 1",
        ])
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .current_dir(&fixture.root)
        .env("HAFLEY_TRACE", &fixture.trace)
        .output()
        .expect("cleave binary runs");
    assert_eq!(output.status.code(), Some(3), "a failed verify exits 3");
    assert_eq!(before, read(&fixture, "src/util.rs"), "the rollback held");
}
