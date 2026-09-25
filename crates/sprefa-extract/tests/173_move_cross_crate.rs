//! `ryi move` and `ryi cleave` across package boundaries: a Rust file leaving
//! one Cargo package for another (`fixtures/rust_cross`, four path crates) and
//! a TS file leaving one package.json for another (`fixtures/ts_cross`).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Fixture {
    root: PathBuf,
    state: PathBuf,
}

fn fixture(name: &str, label: &str) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "ryi_cross_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = base.join("repo");
    let state = base.join("state");
    std::fs::create_dir_all(&state).unwrap();
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/{name}")),
        &root,
    );
    Fixture {
        root: root.canonicalize().unwrap(),
        state,
    }
}

/// Stage `edits` over the copied tree, then commit it as the fixture's history.
fn commit(fixture: &Fixture, edits: &[(&str, &str)]) {
    for (rel, text) in edits {
        std::fs::write(fixture.root.join(rel), text).unwrap();
    }
    git(&fixture.root, &["init", "-q", "."]);
    git(&fixture.root, &["add", "-A"]);
    git(
        &fixture.root,
        &[
            "-c",
            "user.email=ryi@cross.test",
            "-c",
            "user.name=ryi-cross",
            "commit",
            "-qm",
            "fixture",
        ],
    );
}

fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read fixture dir") {
        let entry = entry.expect("fixture entry");
        let to = target.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).expect("copy fixture file");
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

fn ryi(fixture: &Fixture, args: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(args)
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .env("RUST_LOG", "warn")
        .output()
        .expect("ryi runs")
}

fn move_args(fixture: &Fixture, old: &str, new: &str, extra: &[&str]) -> Vec<String> {
    let mut args = vec![
        "move".to_string(),
        fixture.root.join(old).display().to_string(),
        fixture.root.join(new).display().to_string(),
    ];
    args.extend(extra.iter().map(|arg| arg.to_string()));
    args
}

fn read(fixture: &Fixture, rel: &str) -> String {
    std::fs::read_to_string(fixture.root.join(rel)).unwrap_or_else(|error| panic!("read {rel}: {error}"))
}

fn stdout(output: &Output) -> String {
    assert!(
        output.status.success(),
        "ryi exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn cargo_check(fixture: &Fixture) {
    let check = Command::new("cargo")
        .args(["check", "--offline", "-q"])
        .current_dir(&fixture.root)
        .output()
        .expect("cargo runs");
    assert!(
        check.status.success(),
        "cargo check on the moved workspace: {}",
        String::from_utf8_lossy(&check.stderr)
    );
}

// ── Rust ────────────────────────────────────────────────────────────────────

#[test]
fn a_file_crosses_crates_with_paths_manifests_and_visibility() {
    let fixture = fixture("rust_cross", "rust_move");
    commit(&fixture, &[]);
    let table = stdout(&ryi(
        &fixture,
        &move_args(&fixture, "alpha/src/shapes.rs", "beta/src/3_shapes.rs", &["--commit"]),
    ));
    let receipts: Vec<&str> = table
        .lines()
        .filter(|line| {
            ["cross-crate", "widen", "dep ", "relocate"]
                .iter()
                .any(|lead| line.starts_with(lead))
        })
        .collect();
    assert_eq!(
        receipts,
        [
            "cross-crate alpha/src/shapes.rs: alpha -> beta",
            "widen alpha/src/shapes.rs:4 radius -> pub",
            "widen alpha/src/shapes.rs:8 new -> pub",
            "widen alpha/src/shapes.rs:13 area -> pub",
            "dep beta/Cargo.toml: + delta = { path = \"../delta\" } (for alpha/src/shapes.rs)",
            "relocate mod shapes: alpha/src/lib.rs -> beta/src/lib.rs",
            "dep gamma/Cargo.toml: + beta = { path = \"../beta\" } (for gamma/src/lib.rs)",
        ]
    );
    assert_eq!(
        read(&fixture, "alpha/src/lib.rs"),
        "pub mod util;\n\npub use beta::shapes::Circle;\n\npub const VERSION_TAG: &str = \"v1\";\n"
    );
    assert_eq!(
        read(&fixture, "alpha/src/util.rs"),
        "use beta::shapes::perimeter;\n\
         use beta::shapes::{area, Circle};\n\
         use crate::VERSION_TAG;\n\
         \n\
         pub const LABEL: &str = VERSION_TAG;\n\
         \n\
         pub fn unit_area() -> f64 {\n    \
         let circle = Circle::new(1.0);\n    \
         area(&circle) + beta::shapes::area(&circle) + circle.radius + perimeter(1.0)\n\
         }\n"
    );
    assert_eq!(
        read(&fixture, "beta/src/lib.rs"),
        "pub mod base;\n#[path = \"3_shapes.rs\"] pub mod shapes;\n"
    );
    assert_eq!(
        read(&fixture, "beta/src/3_shapes.rs"),
        "use crate::base::Unit;\n\
         \n\
         pub struct Circle {\n    pub radius: f64,\n}\n\
         \n\
         impl Circle {\n    pub fn new(radius: f64) -> Self {\n        Circle { radius }\n    }\n}\n\
         \n\
         pub fn area(circle: &Circle) -> f64 {\n    \
         circle.radius * circle.radius * delta::scale() * Unit::one()\n}\n\
         \n\
         pub fn perimeter(radius: f64) -> f64 {\n    radius * 6.0\n}\n"
    );
    assert_eq!(
        read(&fixture, "beta/Cargo.toml"),
        "[package]\nname = \"beta\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ndelta = { path = \"../delta\" }\n"
    );
    assert_eq!(
        read(&fixture, "gamma/Cargo.toml"),
        "[package]\nname = \"gamma\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nalpha = { path = \"../alpha\" }\nbeta = { path = \"../beta\" }\n"
    );
    assert_eq!(
        read(&fixture, "gamma/src/lib.rs"),
        "use beta::shapes::Circle;\n\npub fn describe(_circle: &Circle) -> &'static str {\n    alpha::util::LABEL\n}\n"
    );
    cargo_check(&fixture);
}

#[test]
fn a_dry_run_names_every_edit_and_touches_nothing() {
    let fixture = fixture("rust_cross", "rust_dry");
    commit(&fixture, &[]);
    let before = read(&fixture, "alpha/src/util.rs");
    let table = stdout(&ryi(
        &fixture,
        &move_args(&fixture, "alpha/src/shapes.rs", "beta/src/shapes.rs", &[]),
    ));
    let edited: Vec<&str> = table
        .lines()
        .filter(|line| line.starts_with("replace ") || line.starts_with("move "))
        .map(|line| line.split_whitespace().nth(1).unwrap_or_default())
        .collect();
    assert_eq!(
        edited,
        [
            "alpha/src/lib.rs",
            "alpha/src/shapes.rs",
            "alpha/src/util.rs",
            "beta/Cargo.toml",
            "beta/src/lib.rs",
            "gamma/Cargo.toml",
            "gamma/src/lib.rs",
            "alpha/src/shapes.rs",
        ]
    );
    assert_eq!(read(&fixture, "alpha/src/util.rs"), before);
    assert!(fixture.root.join("alpha/src/shapes.rs").exists());
}

#[test]
fn a_move_that_closes_a_dependency_cycle_is_a_named_stop() {
    let fixture = fixture("rust_cross", "rust_cycle");
    commit(
        &fixture,
        &[(
            "alpha/src/shapes.rs",
            "pub fn tagged() -> &'static str {\n    crate::VERSION_TAG\n}\n\npub struct Circle;\n\npub fn area(_: &Circle) -> f64 {\n    0.0\n}\n\npub fn perimeter(_: f64) -> f64 {\n    0.0\n}\n",
        )],
    );
    let output = ryi(
        &fixture,
        &move_args(&fixture, "alpha/src/shapes.rs", "beta/src/shapes.rs", &["--commit"]),
    );
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "rust: dependency cycle: beta -> alpha -> beta (the batch makes beta depend on alpha: alpha/src/shapes.rs uses alpha::VERSION_TAG)"
    );
    assert!(fixture.root.join("alpha/src/shapes.rs").exists());
}

#[test]
fn a_module_leaving_its_children_behind_is_a_named_stop() {
    let fixture = fixture("rust_cross", "rust_subtree");
    std::fs::create_dir_all(fixture.root.join("alpha/src/shapes")).unwrap();
    commit(
        &fixture,
        &[
            ("alpha/src/shapes/inner.rs", "pub fn depth() -> u32 {\n    1\n}\n"),
            (
                "alpha/src/shapes.rs",
                "pub mod inner;\n\npub struct Circle;\n\npub fn area(_: &Circle) -> f64 {\n    0.0\n}\n\npub fn perimeter(_: f64) -> f64 {\n    0.0\n}\n",
            ),
        ],
    );
    let output = ryi(
        &fixture,
        &move_args(&fixture, "alpha/src/shapes.rs", "beta/src/shapes.rs", &[]),
    );
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "rust: alpha/src/shapes.rs declares `mod inner` (alpha/src/shapes/inner.rs), which this batch does not move; add it to the batch"
    );
}

#[test]
fn a_cleave_across_crates_spells_the_callers_with_the_crate() {
    let fixture = fixture("rust_cross", "rust_cleave");
    commit(&fixture, &[]);
    let args = vec![
        "cleave".to_string(),
        format!("{}#perimeter", fixture.root.join("alpha/src/shapes.rs").display()),
        fixture.root.join("beta/src/base.rs").display().to_string(),
        "--commit".to_string(),
    ];
    stdout(&ryi(&fixture, &args));
    assert_eq!(
        read(&fixture, "alpha/src/util.rs"),
        "use crate::{shapes::{area, Circle}, VERSION_TAG};\n\
         use beta::base::perimeter;\n\
         \n\
         pub const LABEL: &str = VERSION_TAG;\n\
         \n\
         pub fn unit_area() -> f64 {\n    \
         let circle = Circle::new(1.0);\n    \
         area(&circle) + crate::shapes::area(&circle) + circle.radius + perimeter(1.0)\n\
         }\n"
    );
    assert!(read(&fixture, "beta/src/base.rs").ends_with("pub fn perimeter(radius: f64) -> f64 {\n    radius * 6.0\n}\n"));
    cargo_check(&fixture);
}

#[test]
fn a_batch_carries_a_module_and_its_child_across_crates() {
    let fixture = fixture("rust_cross", "rust_batch");
    std::fs::create_dir_all(fixture.root.join("alpha/src/shapes")).unwrap();
    commit(
        &fixture,
        &[
            (
                "alpha/src/shapes/inner.rs",
                "pub(crate) fn depth() -> u32 {\n    super::super::shapes::perimeter(1.0) as u32\n}\n",
            ),
            (
                "alpha/src/shapes.rs",
                "pub mod inner;\n\npub struct Circle {\n    pub(crate) radius: f64,\n}\n\nimpl Circle {\n    pub(crate) fn new(radius: f64) -> Self {\n        Circle { radius }\n    }\n}\n\npub(crate) fn area(circle: &Circle) -> f64 {\n    circle.radius * f64::from(inner::depth())\n}\n\npub fn perimeter(radius: f64) -> f64 {\n    radius * 6.0\n}\n",
            ),
        ],
    );
    let list = fixture.state.join("moves.tsv");
    std::fs::write(
        &list,
        format!(
            "{}\t{}\n{}\t{}\n",
            fixture.root.join("alpha/src/shapes.rs").display(),
            fixture.root.join("beta/src/shapes.rs").display(),
            fixture.root.join("alpha/src/shapes/inner.rs").display(),
            fixture.root.join("beta/src/shapes/inner.rs").display(),
        ),
    )
    .unwrap();
    let args = vec![
        "move".to_string(),
        "--list".to_string(),
        list.display().to_string(),
        "--commit".to_string(),
    ];
    stdout(&ryi(&fixture, &args));
    assert_eq!(read(&fixture, "beta/src/lib.rs"), "pub mod base;\npub mod shapes;\n");
    assert_eq!(
        read(&fixture, "beta/src/shapes/inner.rs"),
        "pub(crate) fn depth() -> u32 {\n    super::super::shapes::perimeter(1.0) as u32\n}\n"
    );
    assert!(read(&fixture, "beta/src/shapes.rs").starts_with("pub mod inner;\n"));
    cargo_check(&fixture);
}

#[test]
fn a_failed_verify_restores_the_moved_file_bytes() {
    let fixture = fixture("rust_cross", "rust_rollback");
    commit(&fixture, &[]);
    let before = read(&fixture, "alpha/src/shapes.rs");
    let output = ryi(
        &fixture,
        &move_args(
            &fixture,
            "alpha/src/shapes.rs",
            "beta/src/shapes.rs",
            &["--commit", "--verify", "false"],
        ),
    );
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(read(&fixture, "alpha/src/shapes.rs"), before);
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&fixture.root)
        .output()
        .expect("git runs");
    assert_eq!(String::from_utf8_lossy(&status.stdout), "");
}

#[test]
fn a_cleave_into_a_new_numbered_file_declares_it() {
    let fixture = fixture("rust_cross", "rust_cleave_new");
    commit(&fixture, &[]);
    let cross = vec![
        "cleave".to_string(),
        format!("{}#perimeter", fixture.root.join("alpha/src/shapes.rs").display()),
        fixture.root.join("beta/src/4_round.rs").display().to_string(),
        "--commit".to_string(),
    ];
    stdout(&ryi(&fixture, &cross));
    assert_eq!(
        read(&fixture, "beta/src/lib.rs"),
        "pub mod base;\n#[path = \"4_round.rs\"] pub mod round;\n"
    );
    assert!(read(&fixture, "alpha/src/util.rs").contains("use beta::round::perimeter;\n"));
    cargo_check(&fixture);
}

#[test]
fn a_cleave_into_a_new_file_of_the_same_crate_declares_it() {
    let fixture = fixture("rust_cross", "rust_cleave_same");
    commit(&fixture, &[]);
    let args = vec![
        "cleave".to_string(),
        format!("{}#perimeter", fixture.root.join("alpha/src/shapes.rs").display()),
        fixture.root.join("alpha/src/round.rs").display().to_string(),
        "--commit".to_string(),
    ];
    stdout(&ryi(&fixture, &args));
    assert_eq!(
        read(&fixture, "alpha/src/lib.rs"),
        "pub mod shapes;\npub mod util;\npub(crate) mod round;\n\npub use shapes::Circle;\n\npub const VERSION_TAG: &str = \"v1\";\n"
    );
    cargo_check(&fixture);
}

#[test]
fn a_cleave_that_would_cycle_is_a_named_stop() {
    let fixture = fixture("rust_cross", "rust_cleave_cycle");
    commit(&fixture, &[]);
    let args = vec![
        "cleave".to_string(),
        format!("{}#area", fixture.root.join("alpha/src/shapes.rs").display()),
        fixture.root.join("beta/src/base.rs").display().to_string(),
    ];
    let output = ryi(&fixture, &args);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "dependency cycle: alpha -> beta -> alpha (alpha/src/util.rs uses area, and beta/src/base.rs would import from alpha)"
    );
}

// ── TS ──────────────────────────────────────────────────────────────────────

#[test]
fn a_ts_file_crosses_packages_with_package_specifiers_and_a_dependency() {
    let fixture = fixture("ts_cross", "ts_move");
    commit(&fixture, &[]);
    let table = stdout(&ryi(
        &fixture,
        &move_args(&fixture, "packages/a/src/shape.ts", "packages/b/src/shape.ts", &["--commit"]),
    ));
    assert!(
        table.contains("dep packages/a/package.json: + \"@ws/b\": \"*\" (for packages/a/src/util.ts)"),
        "{table}"
    );
    assert_eq!(
        read(&fixture, "packages/a/src/util.ts"),
        "import { area } from '@ws/b/shape';\n\nexport const unitArea = (): number => area(1);\n"
    );
    assert_eq!(
        read(&fixture, "packages/b/src/main.ts"),
        "import { area } from './shape';\n\nexport const twice = (radius: number): number => 2 * area(radius);\n"
    );
    assert_eq!(
        read(&fixture, "packages/a/package.json"),
        "{\n  \"name\": \"@ws/a\",\n  \"version\": \"0.0.0\",\n  \"dependencies\": {\n    \"@ws/b\": \"*\"\n  }\n}\n"
    );
}

#[test]
fn a_ts_move_that_closes_a_package_cycle_is_a_named_stop() {
    let fixture = fixture("ts_cross", "ts_cycle");
    commit(
        &fixture,
        &[(
            "packages/b/package.json",
            "{\n  \"name\": \"@ws/b\",\n  \"version\": \"0.0.0\",\n  \"dependencies\": {\n    \"@ws/a\": \"*\"\n  }\n}\n",
        )],
    );
    let output = ryi(
        &fixture,
        &move_args(&fixture, "packages/a/src/shape.ts", "packages/b/src/shape.ts", &[]),
    );
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "ts: dependency cycle: @ws/a -> @ws/b -> @ws/a (the batch makes @ws/a depend on @ws/b for packages/a/src/util.ts)"
    );
}
