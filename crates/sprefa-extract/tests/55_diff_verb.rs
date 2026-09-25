//! The `extract diff` contract, through the real binary against a real git
//! repository built by `fixtures/diff/make_repo.sh`.
//!
//! The expected rows in `fixtures/diff/expected_*.jsonl` are hand-derived from
//! the three fixture commits: `beta` gains a call to `alpha` (one edge added),
//! `gamma` is renamed to `delta` (one edge removed, one added), and commit 3
//! changes whitespace only in `d.ts` (one digest, zero fact changes). The shas
//! and blob oids in those files are literal because the script pins the commit
//! identity and dates, so the fixture is reproducible byte for byte.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const SCRIPT: &str = "tests/fixtures/diff/make_repo.sh";
const EXPECTED_1_2: &str = include_str!("fixtures/diff/expected_1_2.jsonl");
const EXPECTED_2_3: &str = include_str!("fixtures/diff/expected_2_3.jsonl");

struct TempRoot(PathBuf);

impl TempRoot {
    fn new(tag: &str) -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a post-epoch clock")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("extract_diff_{tag}_{}_{stamp}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("temp root");
        TempRoot(path)
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Repo {
    root: PathBuf,
    temp: TempRoot,
    shas: [String; 3],
}

impl Repo {
    fn build(tag: &str) -> Self {
        let temp = TempRoot::new(tag);
        let root = temp.0.join("repo");
        let output = Command::new("sh")
            .arg(SCRIPT)
            .arg(&root)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("the fixture script runs");
        assert!(
            output.status.success(),
            "fixture script failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("script stdout is UTF-8");
        let shas: Vec<String> = stdout.lines().map(str::to_string).collect();
        assert_eq!(shas.len(), 3, "script prints three shas, got {stdout:?}");
        Repo {
            root,
            temp,
            shas: [shas[0].clone(), shas[1].clone(), shas[2].clone()],
        }
    }

    fn diff(&self, from: &str, to: &str, extra: &[&str]) -> std::process::Output {
        let mut args: Vec<String> = vec![
            "diff".to_string(),
            "--root".to_string(),
            self.root.to_string_lossy().to_string(),
            "--from".to_string(),
            from.to_string(),
            "--to".to_string(),
            to.to_string(),
        ];
        args.extend(extra.iter().map(|arg| arg.to_string()));
        Command::new(env!("CARGO_BIN_EXE_ryi"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(&args)
            .output()
            .expect("the extract binary runs")
    }

    fn stdout(&self, from: &str, to: &str) -> String {
        let output = self.diff(from, to, &[]);
        assert!(
            output.status.success(),
            "diff {from} {to} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("stdout is UTF-8")
    }

    fn header(&self, from: &str, to: &str) -> Value {
        let stdout = self.stdout(from, to);
        serde_json::from_str(stdout.lines().next().expect("a header line"))
            .expect("the header is JSON")
    }
}

fn records(stdout: &str) -> Vec<Value> {
    stdout
        .lines()
        .map(|line| serde_json::from_str(line).expect("every diff line is JSON"))
        .collect()
}

/// (a) The full 1 -> 2 delta, byte for byte against rows derived by hand from
/// the fixture source: two changed blobs, one edge added, one removed, one
/// added back under the new name.
#[test]
fn one_to_two_matches_the_hand_derived_rows() {
    let repo = Repo::build("one_two");
    assert_eq!(repo.stdout(&repo.shas[0], &repo.shas[1]), EXPECTED_1_2);
}

/// (b) Commit 3 is whitespace only: one blob digest moves and no fact does, so
/// the header reports the blob and the stream carries zero edge rows.
#[test]
fn two_to_three_changes_one_blob_and_no_edge() {
    let repo = Repo::build("two_three");
    let stdout = repo.stdout(&repo.shas[1], &repo.shas[2]);
    assert_eq!(stdout, EXPECTED_2_3);
    let header = repo.header(&repo.shas[1], &repo.shas[2]);
    assert_eq!(header["changed_blobs"], 1);
    assert_eq!(header["counts"]["resolved_edge"]["added"], 0);
    assert_eq!(header["counts"]["resolved_edge"]["removed"], 0);
    assert_eq!(
        records(&stdout)
            .iter()
            .filter(|row| row["record"] == "diff_edge")
            .count(),
        0
    );
}

/// (c) A revision against itself is the header and nothing else.
#[test]
fn the_same_revision_is_the_header_alone() {
    let repo = Repo::build("same");
    let stdout = repo.stdout(&repo.shas[1], &repo.shas[1]);
    assert_eq!(stdout.lines().count(), 1);
    let header = repo.header(&repo.shas[1], &repo.shas[1]);
    assert_eq!(header["from"], header["to"]);
    assert_eq!(header["files_a"], 4);
    assert_eq!(header["files_b"], 4);
    assert_eq!(header["changed_blobs"], 0);
    for relation in [
        "file",
        "resolved_edge",
        "resolved_type_edge",
        "resolved_import",
        "file_unresolved",
        "unresolved",
    ] {
        for change in ["added", "removed", "changed", "origin_changed"] {
            assert_eq!(header["counts"][relation][change], 0, "{relation} {change}");
        }
    }
}

/// (d) The `--sqlite` law: an existing path is refused, the run exits nonzero,
/// and neither that file nor a staging file beside it is written.
#[test]
fn sqlite_refuses_an_existing_path_and_writes_nothing() {
    let repo = Repo::build("sqlite");
    let taken = repo.temp.0.join("taken.db");
    std::fs::write(&taken, "sentinel\n").expect("write the sentinel");
    let before: Vec<String> = entries(&repo.temp.0);

    let output = repo.diff(
        &repo.shas[0],
        &repo.shas[1],
        &["--sqlite", taken.to_string_lossy().as_ref()],
    );
    assert!(!output.status.success(), "an existing path must be refused");
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("already exists"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&taken).expect("read the sentinel"),
        "sentinel\n"
    );
    assert_eq!(entries(&repo.temp.0), before, "no staging file may survive");
}

fn entries(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .expect("read the temp root")
        .map(|entry| entry.expect("a directory entry").file_name())
        .map(|name| name.to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

/// (e) Two runs on the same shas are byte-identical: the row order is a total
/// order over (record, path, key) and nothing in the wire is machine-local.
#[test]
fn two_runs_are_byte_identical() {
    let repo = Repo::build("determinism");
    let first = repo.stdout(&repo.shas[0], &repo.shas[1]);
    let second = repo.stdout(&repo.shas[0], &repo.shas[1]);
    assert_eq!(first, second);
    assert!(first.ends_with('\n'));
}

/// (f) Blob bytes come from `git cat-file`, never from the checkout: a dirty
/// worktree changes not one byte of the delta.
#[test]
fn a_dirty_worktree_does_not_change_the_delta() {
    let repo = Repo::build("dirty");
    let clean = repo.stdout(&repo.shas[0], &repo.shas[1]);
    let checked_out = repo.root.join("b.ts");
    std::fs::write(
        &checked_out,
        "export function beta(): number {\n  return 999;\n}\nexport function dirty(): void {}\n",
    )
    .expect("dirty the worktree");
    let status = Command::new("git")
        .arg("-C")
        .arg(&repo.root)
        .args(["status", "--porcelain"])
        .output()
        .expect("git status runs");
    assert!(
        !status.stdout.is_empty(),
        "the worktree must be dirty for this proof to mean anything"
    );
    let dirty = repo.stdout(&repo.shas[0], &repo.shas[1]);
    assert_eq!(dirty, clean);
    assert_eq!(dirty, EXPECTED_1_2);
}
