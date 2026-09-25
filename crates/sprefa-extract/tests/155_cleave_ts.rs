//! `ryi cleave` on the TS arm, over `tests/fixtures/cleave_ts`: the three-file
//! step trace from `plans/2026-09-20-graph-views-and-cleave.md`, plus the one
//! and two level drag variants.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Fixture {
    root: PathBuf,
    state: PathBuf,
    trace: PathBuf,
}

fn fixture(variant: &str, label: &str) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "ryi_cleave_{variant}_{label}_{}_{}",
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
        .join("tests/fixtures/cleave_ts")
        .join(variant);
    copy_tree(&source, &root);
    git(&root, &["init", "-q", "."]);
    Fixture {
        root: root.canonicalize().unwrap(),
        state,
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

/// Every file under `root`, root-relative, with its bytes. A dry run leaves
/// this map byte-identical.
fn digest(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("read dir") {
            let path = entry.expect("entry").path();
            if path.file_name().is_some_and(|name| name == ".git") {
                continue;
            }
            match path.is_dir() {
                true => pending.push(path),
                false => {
                    let rel = path.strip_prefix(root).unwrap().to_string_lossy().into();
                    out.insert(rel, std::fs::read(&path).expect("read file"));
                }
            }
        }
    }
    out
}

fn run_cleave(fixture: &Fixture, args: &[&str]) -> (Option<i32>, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .arg("cleave")
        .args(args)
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .current_dir(&fixture.root)
        .env("HAFLEY_TRACE", &fixture.trace)
        .env("RUST_LOG", "sprefa_extract=debug,hafley_scm=debug")
        .output()
        .expect("cleave binary runs");
    (
        output.status.code(),
        String::from_utf8(output.stdout).expect("cleave stdout is UTF-8"),
    )
}

fn cleave(fixture: &Fixture, args: &[&str]) -> String {
    let (code, stdout) = run_cleave(fixture, args);
    assert_eq!(code, Some(0), "cleave {args:?} exited {code:?}:\n{stdout}");
    stdout
}

/// `import` lines in one root-relative file.
fn import_lines(fixture: &Fixture, rel: &str) -> usize {
    std::fs::read_to_string(fixture.root.join(rel))
        .expect("read fixture file")
        .lines()
        .filter(|line| line.starts_with("import "))
        .count()
}

fn occurrences(fixture: &Fixture, rel: &str, needle: &str) -> usize {
    std::fs::read_to_string(fixture.root.join(rel))
        .expect("read fixture file")
        .matches(needle)
        .count()
}

/// The one `cleave_plan` line `--json` closes with.
fn plan_of(stdout: &str) -> serde_json::Value {
    let line = stdout
        .lines()
        .find(|line| line.starts_with('{'))
        .expect("a cleave_plan json line");
    serde_json::from_str(line).expect("cleave_plan parses")
}

/// Chrome `B` events in the run's trace file. Spans are `B`/`E` pairs, so one
/// `B` is one span the run opened.
fn trace_spans(fixture: &Fixture) -> usize {
    let text = std::fs::read_to_string(&fixture.trace).expect("trace file");
    let events: Vec<serde_json::Value> = serde_json::from_str(&text).expect("trace parses");
    events
        .iter()
        .filter(|event| event["ph"].as_str() == Some("B"))
        .count()
}

fn names(plan: &serde_json::Value, key: &str) -> Vec<String> {
    plan[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} is an array"))
        .iter()
        .map(|row| row["name"].as_str().expect("a name").to_string())
        .collect()
}

#[test]
fn plan_partitions_the_three_file_step_trace() {
    let fixture = fixture("basic", "plan");
    let before = digest(&fixture.root);
    let stdout = cleave(&fixture, &["src/util.ts#loadConfig", "src/config.ts", "--json"]);
    let plan = plan_of(&stdout);

    assert_eq!(plan["record"], "cleave_plan");
    assert_eq!(plan["src"], "src/util.ts");
    assert_eq!(plan["dest"], "src/config.ts");
    assert_eq!(plan["item"], "loadConfig");
    assert_eq!(names(&plan, "travelling"), ["readFileSync", "join", "LOG"]);
    assert_eq!(names(&plan, "orphans"), ["readFileSync", "join", "LOG"]);
    assert_eq!(
        plan["callers"].as_array().unwrap(),
        &vec![serde_json::json!("src/app.ts")]
    );
    assert_eq!(plan["drag_iterations"], 1);
    assert_eq!(plan["unresolved"].as_array().unwrap().len(), 0);

    // `join` already sits in config.ts, so it travels as `carried` and lands
    // nowhere.
    let kinds: Vec<&str> = plan["travelling"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["package", "carried", "relative"]);

    assert_eq!(before, digest(&fixture.root), "a dry run writes no byte");
    assert!(trace_spans(&fixture) > 0, "the run emitted no chrome spans");
}

#[test]
fn commit_moves_the_item_and_its_imports() {
    let fixture = fixture("basic", "commit");
    assert_eq!(import_lines(&fixture, "src/util.ts"), 3);
    assert_eq!(import_lines(&fixture, "src/config.ts"), 1);
    assert_eq!(import_lines(&fixture, "src/app.ts"), 1);

    cleave(
        &fixture,
        &["src/util.ts#loadConfig", "src/config.ts", "--commit"],
    );

    assert_eq!(import_lines(&fixture, "src/util.ts"), 0, "3 imports left");
    assert_eq!(import_lines(&fixture, "src/config.ts"), 3, "2 imports came");
    assert_eq!(import_lines(&fixture, "src/app.ts"), 2, "1 import came");
    assert_eq!(
        occurrences(&fixture, "src/config.ts", "node:path"),
        1,
        "config.ts already carried join"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("src/util.ts")).unwrap(),
        "export function slug(raw: string): string { return raw.toLowerCase(); }\n"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("src/app.ts")).unwrap(),
        "import { slug } from \"./util\";\n\
         import { loadConfig } from \"./config\";\n\
         export function boot(dir: string) { return slug(loadConfig(dir)); }\n"
    );
}

#[test]
fn a_missing_destination_is_created_with_every_specifier() {
    let fixture = fixture("basic", "create");
    cleave(
        &fixture,
        &["src/util.ts#loadConfig", "src/loader.ts", "--commit"],
    );
    assert_eq!(import_lines(&fixture, "src/loader.ts"), 3);
    assert_eq!(
        occurrences(&fixture, "src/app.ts", "\"./loader\""),
        1,
        "the caller aims at the new file"
    );
}

#[test]
fn a_failed_verify_rolls_all_three_files_back() {
    let fixture = fixture("basic", "verify");
    let before = digest(&fixture.root);
    let (code, stdout) = run_cleave(
        &fixture,
        &[
            "src/util.ts#loadConfig",
            "src/config.ts",
            "--commit",
            "--verify",
            "exit 1",
        ],
    );
    assert_eq!(code, Some(3), "a failed verify exits 3:\n{stdout}");
    assert!(
        stdout.contains("verify failed (rc=1): rolled back 3 files"),
        "no rollback receipt:\n{stdout}"
    );
    assert_eq!(before, digest(&fixture.root), "the rollback left a byte");
}

/// The `action` of every dragged helper, in plan order.
fn actions(plan: &serde_json::Value) -> Vec<&str> {
    plan["dragged"]
        .as_array()
        .expect("dragged is an array")
        .iter()
        .map(|row| row["action"].as_str().expect("an action"))
        .collect()
}

#[test]
fn drag_decides_moving_not_whether_the_helper_is_reachable() {
    let plain = fixture("drag", "plain");
    let plan = plan_of(&cleave(
        &plain,
        &["src/util.ts#loadConfig", "src/config.ts", "--json"],
    ));
    assert_eq!(names(&plan, "dragged"), ["slug"]);
    assert_eq!(actions(&plan), ["exported"], "no --drag exports, never moves");
    assert_eq!(plan["drag_iterations"], 1);

    let dragged = fixture("drag", "on");
    let plan = plan_of(&cleave(
        &dragged,
        &["src/util.ts#loadConfig", "src/config.ts", "--drag", "--json"],
    ));
    assert_eq!(names(&plan, "dragged"), ["slug"]);
    assert_eq!(actions(&plan), ["moved"], "1 drag candidate under --drag");
    assert_eq!(plan["drag_iterations"], 1);
}

#[test]
fn a_helper_the_source_still_uses_is_exported_not_moved() {
    let fixture = fixture("drag_shared", "split");
    let plan = plan_of(&cleave(
        &fixture,
        &["src/util.ts#loadConfig", "src/config.ts", "--drag", "--json"],
    ));
    assert_eq!(names(&plan, "dragged"), ["pad", "slug"]);
    assert_eq!(actions(&plan), ["exported", "moved"]);

    cleave(
        &fixture,
        &[
            "src/util.ts#loadConfig",
            "src/config.ts",
            "--drag",
            "--commit",
        ],
    );
    let util = std::fs::read_to_string(fixture.root.join("src/util.ts")).unwrap();
    assert!(util.contains("export function pad("), "pad gained an export");
    assert!(!util.contains("function slug("), "slug left");
    assert_eq!(occurrences(&fixture, "src/config.ts", "function slug("), 1);
    assert_eq!(
        occurrences(&fixture, "src/config.ts", "import { pad } from \"./util\""),
        1,
        "the destination imports the shared helper from the source"
    );
    assert_eq!(
        occurrences(&fixture, "src/config.ts", "function pad("),
        0,
        "a helper is never copied"
    );
}

#[test]
fn a_non_typescript_source_names_the_out_of_scope_list() {
    let fixture = fixture("basic", "scope");
    std::fs::write(fixture.root.join("src/lib.rs"), "pub fn boot() {}\n").unwrap();
    let (code, _) = run_cleave(&fixture, &["src/lib.rs#boot", "src/other.rs"]);
    assert_eq!(code, Some(2), "a path no arm owns is a plan error");
}

#[test]
fn the_drag_fixpoint_reports_its_pass_count() {
    let fixture = fixture("drag_two_level", "fixpoint");
    let plan = plan_of(&cleave(
        &fixture,
        &["src/util.ts#loadConfig", "src/config.ts", "--drag", "--json"],
    ));
    assert_eq!(names(&plan, "dragged"), ["slug", "tidy"]);
    assert_eq!(
        plan["dragged"][0]["iteration"], 1,
        "the item's own reference is pass 1"
    );
    assert_eq!(
        plan["dragged"][1]["iteration"], 2,
        "a helper reached through a helper is pass 2"
    );
    assert_eq!(plan["drag_iterations"], 2);
}

#[test]
fn a_dragged_helper_lands_in_the_destination() {
    let fixture = fixture("drag_two_level", "apply");
    cleave(
        &fixture,
        &[
            "src/util.ts#loadConfig",
            "src/config.ts",
            "--drag",
            "--commit",
        ],
    );
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("src/util.ts")).unwrap(),
        "export const UTIL_VERSION = 1;\n"
    );
    assert_eq!(occurrences(&fixture, "src/config.ts", "function tidy"), 1);
    assert_eq!(occurrences(&fixture, "src/config.ts", "function slug"), 1);
    assert_eq!(import_lines(&fixture, "src/config.ts"), 1, "LOG came along");
}
