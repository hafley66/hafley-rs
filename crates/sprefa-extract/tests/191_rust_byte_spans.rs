//! Every Rust span is a UTF-8 byte offset, on a line where `é` and `—` precede
//! the name: the checker's call answers and both rename tiers.

#![cfg(feature = "rust-checker")]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const DIR: &str = "tests/fixtures/rust_non_ascii";

fn source(file: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(DIR).join(file)).unwrap()
}

fn resolve() -> Vec<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("RUST_LOG", "off")
        .args(["--resolve", "--arms", "call", "--root", DIR, "--rust-checker"])
        .arg(format!("{DIR}/src/lib.rs"))
        .arg(format!("{DIR}/src/words.rs"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn checker_call_sites_are_byte_offsets() {
    let lib = source("src/lib.rs");
    let words = source("src/words.rs");
    let at = |text: &str, needle: &str, nth: usize| text.match_indices(needle).nth(nth).unwrap().0;
    let rows = resolve();
    let mut edges: Vec<(String, usize, String, String)> = rows
        .iter()
        .filter(|row| row["record"] == "resolved_edge")
        .map(|row| {
            (
                row["caller_path"].as_str().unwrap().rsplit('/').next().unwrap().to_string(),
                row["caller_site_start"].as_u64().unwrap() as usize,
                row["callee_name"].as_str().unwrap().to_string(),
                row["resolution_origin"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    edges.sort();
    let edge = |file: &str, start: usize, callee: &str| {
        (file.to_string(), start, callee.to_string(), "checker".to_string())
    };
    let mut expected = vec![
        edge("lib.rs", at(&lib, "words::count_words", 0), "count_words"),
        edge("lib.rs", at(&lib, "words::count_words", 1), "count_words"),
        edge("lib.rs", at(&lib, "words::twice", 0), "twice"),
        edge("words.rs", at(&words, "count_words(text)", 0), "count_words"),
        edge("words.rs", at(&words, "count_words(\"", 0), "count_words"),
    ];
    expected.sort();
    assert_eq!(edges, expected);
    let unresolved: Vec<(u64, u64, String)> = rows
        .iter()
        .filter(|row| row["record"] == "unresolved")
        .map(|row| {
            (
                row["span"]["start"].as_u64().unwrap(),
                row["span"]["end"].as_u64().unwrap(),
                row["detail"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let len = at(&words, "len()", 0) as u64;
    assert_eq!(unresolved, vec![(len, len + 3, "core::str::len".to_string())]);
}

struct Fixture {
    root: PathBuf,
    state: PathBuf,
    _scratch: tempfile::TempDir,
}

fn fixture() -> Fixture {
    let scratch = tempfile::Builder::new().prefix("ryi_byte_spans_").tempdir().unwrap();
    let root = scratch.path().join("repo");
    let state = scratch.path().join("state");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(&state).unwrap();
    for file in ["Cargo.toml", "Cargo.lock", "src/lib.rs", "src/words.rs"] {
        std::fs::write(root.join(file), source(file)).unwrap();
    }
    Fixture {
        root: root.canonicalize().unwrap(),
        state,
        _scratch: scratch,
    }
}

fn rename_count_words(slow: bool) {
    let fixture = fixture();
    let words = source("src/words.rs");
    let at = words.find("pub fn count_words").unwrap() + "pub fn ".len();
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(&fixture.root)
        .env("RUST_LOG", "off")
        .args(["rename", "--commit", "--at", &at.to_string()])
        .args(slow.then_some("--slow"))
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .args(["src/words.rs#count_words", "tally"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "rename exited {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    for file in ["src/lib.rs", "src/words.rs"] {
        assert_eq!(
            std::fs::read_to_string(fixture.root.join(file)).unwrap(),
            source(file).replace("count_words", "tally"),
            "{file}"
        );
    }
}

#[test]
fn fast_rename_after_non_ascii_is_byte_exact() {
    rename_count_words(false);
}

#[test]
fn slow_rename_after_non_ascii_is_byte_exact() {
    rename_count_words(true);
}
