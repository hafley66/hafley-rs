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
    assert!(
        output.status.success(),
        "cleave {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("cleave stdout is UTF-8")
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
