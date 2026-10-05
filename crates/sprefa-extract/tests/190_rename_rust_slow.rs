//! `rename --slow` on Rust is rust-analyzer's rename, over the three shapes the
//! tokio corpus broke in the fast tier; each run is proved by `cargo check`.

#![cfg(feature = "rust-checker")]

use std::path::Path;
use std::process::Command;

struct Fixture {
    root: std::path::PathBuf,
    state: std::path::PathBuf,
    _scratch: tempfile::TempDir,
}

fn fixture() -> Fixture {
    let scratch = tempfile::Builder::new()
        .prefix("ryi_rename_rust_slow_")
        .tempdir()
        .unwrap();
    let root = scratch.path().join("repo");
    let state = scratch.path().join("state");
    std::fs::create_dir_all(&state).unwrap();
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rust_rename_slow"),
        &root,
    );
    Fixture {
        root: root.canonicalize().unwrap(),
        state,
        _scratch: scratch,
    }
}

fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let to = target.join(entry.file_name());
        match entry.file_type().unwrap().is_dir() {
            true => copy_tree(&entry.path(), &to),
            false => drop(std::fs::copy(entry.path(), &to).unwrap()),
        }
    }
}

/// Rename the declaration of `old` whose text starts at `declaration` in `file`.
fn rename(fixture: &Fixture, file: &str, declaration: &str, old: &str, new: &str) {
    rename_in(fixture, file, declaration, old, new, true)
}

fn rename_in(fixture: &Fixture, file: &str, declaration: &str, old: &str, new: &str, slow: bool) {
    let text = std::fs::read_to_string(fixture.root.join(file)).unwrap();
    let at = text.find(declaration).unwrap() + declaration.find(old).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(&fixture.root)
        .env("RUST_LOG", "off")
        .args(["rename", "--commit", "--at", &at.to_string()])
        .args(slow.then_some("--slow"))
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .arg(format!("{file}#{old}"))
        .arg(new)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "rename exited {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let check = Command::new(env!("CARGO"))
        .args(["check", "--quiet", "--offline"])
        .current_dir(&fixture.root)
        .env("CARGO_TARGET_DIR", fixture.root.join("target"))
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "cargo check after renaming {old}: {}",
        String::from_utf8_lossy(&check.stderr)
    );
}

fn read(fixture: &Fixture, file: &str) -> String {
    std::fs::read_to_string(fixture.root.join(file)).unwrap()
}

#[test]
fn a_function_named_like_its_module_renames_without_the_module() {
    let fixture = fixture();
    rename(&fixture, "src/fs/copy.rs", "pub fn copy", "copy", "duplicate");
    assert_eq!(
        read(&fixture, "src/fs/mod.rs"),
        "mod copy;\npub use self::copy::duplicate;\n\npub fn copy_twice() -> u32 {\n    duplicate() + self::copy::duplicate()\n}\n"
    );
}

#[test]
fn one_of_two_sibling_types_renames_alone() {
    let fixture = fixture();
    rename(&fixture, "src/runtime/scheduler.rs", "pub struct Context", "Context", "Scheduler");
    assert_eq!(
        read(&fixture, "src/runtime/mod.rs"),
        "pub mod scheduler;\npub mod trace;\n\npub fn with_both(scheduler: &scheduler::Scheduler, trace: &trace::Context) -> u32 {\n    scheduler.id + trace.depth\n}\n"
    );
    assert!(read(&fixture, "src/runtime/trace.rs").contains("pub struct Context"));
}

#[test]
fn a_method_renames_apart_from_the_free_function_it_calls() {
    let fixture = fixture();
    rename(&fixture, "src/queue.rs", "pub fn len", "len", "count");
    let queue = read(&fixture, "src/queue.rs");
    assert!(queue.contains("fn len(items: &[u32])"), "{queue}");
    assert!(queue.contains("pub fn count(&self)"), "{queue}");
    assert!(queue.contains("len(&self.items)"), "{queue}");
    assert!(queue.contains("self.count() == 0"), "{queue}");
}

#[test]
fn fast_tier_keeps_the_module_segment_and_renames_the_bare_call() {
    let fixture = fixture();
    rename_in(&fixture, "src/fs/copy.rs", "pub fn copy", "copy", "duplicate", false);
    assert_eq!(
        read(&fixture, "src/fs/mod.rs"),
        "mod copy;\npub use self::copy::duplicate;\n\npub fn copy_twice() -> u32 {\n    duplicate() + self::copy::duplicate()\n}\n"
    );
}

#[test]
fn fast_tier_renames_one_of_two_sibling_types() {
    let fixture = fixture();
    rename_in(&fixture, "src/runtime/scheduler.rs", "pub struct Context", "Context", "Scheduler", false);
    assert!(read(&fixture, "src/runtime/trace.rs").contains("pub struct Context"));
    assert!(read(&fixture, "src/runtime/mod.rs").contains("&scheduler::Scheduler, trace: &trace::Context"));
}

#[test]
fn fast_tier_stops_on_a_method_call_whose_receiver_type_it_cannot_see() {
    let fixture = fixture();
    let text = read(&fixture, "src/queue.rs");
    let at = text.find("pub fn len").unwrap() + "pub fn ".len();
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(&fixture.root)
        .env("RUST_LOG", "off")
        .args(["rename", "--commit", "--at", &at.to_string(), "--root"])
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .arg("src/queue.rs#len")
        .arg("count")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(6));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("src/queue.rs:6: method call on a receiver of unknown type"), "{stderr}");
    assert_eq!(read(&fixture, "src/queue.rs"), text);
}
