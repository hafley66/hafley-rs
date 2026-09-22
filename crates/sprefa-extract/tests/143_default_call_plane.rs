//! The default family set, ONE for every language (`FamilyMask::DEFAULT`,
//! types.rs): `cst` is opt-in, the default stream carries call sites and never
//! the tree, and a file that yields zero facts discloses instead of streaming
//! silence. The rule has no branch on language or file content, so every rail
//! here holds for any two languages alike.
//!
//! The bash fixtures that drove the old ast-grep guessed-call plane died with
//! that dependency (no bash grammar is linked); python carries the same
//! observable rails, and html is the linked-grammar fallback that discloses
//! matched-but-empty.
#![cfg(feature = "cli")]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn ryi(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(args)
        .env("RUST_LOG", "off")
        .output()
        .expect("run ryi")
}

fn stdout_lines(output: &std::process::Output) -> Vec<String> {
    assert!(
        output.status.success(),
        "ryi failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn write_fixture(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, content).expect("write fixture");
    path
}

fn records(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            let value: Value = serde_json::from_str(line).expect("each row is json");
            value["record"].as_str().expect("record field").to_string()
        })
        .collect()
}

#[test]
fn default_streams_call_sites_and_never_the_tree() {
    let dir = scratch("default-call-plane");
    let py = write_fixture(
        &dir,
        "pipeline.py",
        "def build_step(item):\n    return item\n\n\nbuild_step(1)\ncat('manifest')\n",
    );
    let path = py.to_str().expect("utf8 path");
    let lines = stdout_lines(&ryi(&[path]));
    // Not a parse tree: not one cst row anywhere in the default stream.
    assert!(
        !lines.iter().any(|line| line.contains("\"family\":\"cst\"")),
        "cst rows leaked into the default stream"
    );
    // Sites stream: the call plane rides the default mask, the tree does not.
    let site_lines: Vec<&String> = lines
        .iter()
        .filter(|line| {
            let value: Value = serde_json::from_str(line).expect("each row is json");
            value["record"] == "site"
        })
        .collect();
    assert!(
        !site_lines.is_empty(),
        "the default stream carries call sites: {lines:?}"
    );
    let mut callees: Vec<String> = site_lines
        .iter()
        .map(|line| {
            let value: Value = serde_json::from_str(line.as_str()).expect("site row is json");
            value["callee"].as_str().expect("callee field").to_string()
        })
        .collect();
    callees.sort();
    assert_eq!(
        callees,
        vec!["build_step", "cat"],
        "the call-kind nodes, one site each"
    );
}

#[test]
fn default_plus_cst_partitions_the_full_mask() {
    let dir = scratch("default-partition");
    let py = write_fixture(
        &dir,
        "partition.py",
        "def deploy(item):\n    return item\n\n\ndeploy('now')\n",
    );
    let ts = write_fixture(
        &dir,
        "partition.ts",
        "export function hop(n: number): number { return n + 1; }\nhop(2);\n",
    );
    for fixture in [&py, &ts] {
        let path = fixture.to_str().expect("utf8 path");
        let mut both = stdout_lines(&ryi(&[path]));
        both.extend(stdout_lines(&ryi(&["--family", "cst", path])));
        both.sort();
        let full = stdout_lines(&ryi(&["--family", "cst,type,call,df,data", path]));
        let mut full = full;
        full.sort();
        assert_eq!(
            both, full,
            "{path}: the default set plus opt-in cst must be exactly the full mask"
        );
    }
}

#[test]
fn zero_facts_prints_the_disclosure_block_and_exits_zero() {
    let dir = scratch("default-disclosure");
    let unknown = write_fixture(&dir, "blob.xyz", "???");
    let output = ryi(&[unknown.to_str().expect("utf8 path")]);
    assert!(output.status.success(), "a zero-fact file exits 0");
    assert!(
        output.stdout.is_empty(),
        "the disclosure keeps stdout clean for the pipe"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut block = stderr.lines().filter(|line| !line.is_empty());
    assert_eq!(
        block.next(),
        Some("0 facts. No Source matches .xyz."),
        "the no-Source disclosure names the extension"
    );
    assert_eq!(
        block.next().map(str::trim_end),
        Some(
            format!(
                "  ryi --family cst {}    the parse tree, if a grammar loaded",
                unknown.display()
            )
            .as_str(),
        ),
    );
    assert_eq!(
        block.next(),
        Some("  ryi --schema               which extensions have a Source"),
        "the schema command that would answer"
    );

    // A Source that matches but yields none discloses the same way. css lost
    // its grammar with the ast-grep unlink, so it now answers on the no-Source
    // branch; html is the linked fallback and it matched, empty.
    let css = write_fixture(&dir, "flat.css", ".a { color: red; }\n");
    let output = ryi(&[css.to_str().expect("utf8 path")]);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("0 facts. No Source matches .css."),
        "a grammar-less extension discloses the no-Source branch: {stderr}"
    );

    let html = write_fixture(
        &dir,
        "flat.html",
        "<html><body><p>text</p></body></html>\n",
    );
    let output = ryi(&[html.to_str().expect("utf8 path")]);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("0 facts. fallback matched "),
        "the matched-but-empty disclosure names the Source: {stderr}"
    );
    assert!(
        stderr.contains("--family cst") && stderr.contains("--schema"),
        "the disclosure still names the two commands: {stderr}"
    );
}

#[test]
fn sites_speak_the_existing_resolution_vocabulary() {
    let dir = scratch("default-resolve");
    let caller = write_fixture(
        &dir,
        "caller.py",
        "import helpers\n\nhelpers.helper(1)\nstray(2)\n",
    );
    let helpers = write_fixture(&dir, "helpers.py", "def helper(x):\n    return x\n");
    let caller_path = caller.to_str().expect("utf8 path");

    // Alone, the unbound site stays silent for a language whose arm carries no
    // drop channel; nothing binds, so nothing prints but the trace.
    let alone = stdout_lines(&ryi(&["--resolve", caller_path]));
    assert!(
        !alone
            .iter()
            .any(|line| line.contains("\"record\":\"resolved_edge\"")),
        "no corpus def, no bound edge: {alone:?}"
    );

    // One python def named `helper`: the site binds corpus_unique, an
    // ordinary resolved_edge with no new kind and no new origin.
    let helpers_path = helpers.to_str().expect("utf8 path");
    let bound = stdout_lines(&ryi(&["--resolve", caller_path, helpers_path]));
    let edges: Vec<Value> = bound
        .iter()
        .map(|line| serde_json::from_str(line).expect("row is json"))
        .filter(|value: &Value| value["record"] == "resolved_edge")
        .collect();
    let helper_edges: Vec<&Value> = edges
        .iter()
        .filter(|edge| edge["callee_name"] == "helper")
        .collect();
    assert_eq!(
        helper_edges.len(),
        1,
        "exactly the helper site binds: {edges:?}"
    );
    assert_eq!(helper_edges[0]["kind"], "name_resolve");
    assert_eq!(helper_edges[0]["resolution_origin"], "corpus_unique");
}
