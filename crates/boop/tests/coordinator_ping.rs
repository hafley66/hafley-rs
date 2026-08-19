//! End-to-end coordinator ping delivery over a real tmux session.
//!
//! FAIL-PRE-FIX: `boop adopt` wrote `kind: "lane"`, so `boop hail` took the
//! "lane supervisor delivers it" branch and every result row addressed to an
//! adopted coordinator queued in bus.ndjson forever, never reaching the pane.
use std::path::{Path, PathBuf};
use std::process::Command;

const BOOP: &str = env!("CARGO_BIN_EXE_boop");

fn mail_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("boop-ping-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(dir.join("home")).unwrap();
    dir
}

fn boop(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(BOOP)
        .args(args)
        .arg("--mail-dir")
        .arg(dir)
        // A store of this test's own. Pointed at the machine's live
        // ~/.agent/boop.db, `hail`'s control-edge write raced the user's own
        // boop processes and died on `database is locked` (SQLite code 5) in 3
        // of 5 whole-suite runs: 374MB, `journal_mode=delete`, a 5s busy_timeout
        // and writers holding longer than that.
        .env("BOOP_DB", dir.join("boop.db"))
        .env("HOME", dir.join("home"))
        .output()
        .unwrap()
}

fn tmux(args: &[&str]) -> std::process::Output {
    Command::new("tmux").args(args).output().unwrap()
}

struct TestSession(String);

impl TestSession {
    fn new(name: &str) -> Self {
        let session = format!("boop-ping-{}-{name}", std::process::id());
        let _ = tmux(&["kill-session", "-t", &session]);
        let output = tmux(&["new-session", "-d", "-s", &session]);
        assert!(
            output.status.success(),
            "tmux new-session failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        TestSession(session)
    }
}

impl Drop for TestSession {
    fn drop(&mut self) {
        let _ = tmux(&["kill-session", "-t", &self.0]);
    }
}

fn route_kind(dir: &Path, name: &str) -> String {
    let raw = std::fs::read_to_string(dir.join("registry.json")).unwrap();
    let map: serde_json::Value = serde_json::from_str(&raw).unwrap();
    map[name]["kind"].as_str().unwrap_or_default().to_owned()
}

#[test]
fn adopt_writes_a_coordinator_route() {
    let dir = mail_dir("adopt");
    let session = TestSession::new("adopt");
    // `--cwd` is not optional in a test: without it, adopting a claude session
    // installs the hook inbox into whatever directory the test binary ran in,
    // which is the crate's own source tree.
    let output = boop(
        &dir,
        &[
            "adopt",
            "--name",
            "test-coord",
            "--tmux",
            &session.0,
            "--harness",
            "claude",
            "--cwd",
            dir.to_str().unwrap(),
        ],
    );
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    assert_eq!(route_kind(&dir, "test-coord"), "coordinator");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn lane_patch_still_writes_a_lane_route() {
    let dir = mail_dir("patch");
    let session = TestSession::new("patch");
    let output = boop(
        &dir,
        &["beep", "lane", "patch", "test-lane", "--tmux", &session.0],
    );
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    assert_eq!(route_kind(&dir, "test-lane"), "lane");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The full delivery path a finishing lane's epilogue exercises: hail resolves
/// the coordinator route, the claude harness types the line into the pane.
#[test]
fn hail_to_an_adopted_coordinator_lands_in_its_pane() {
    let dir = mail_dir("deliver");
    let session = TestSession::new("deliver");
    // `--no-hooks` is the point: pane injection is now the opt-out path, since a
    // claude coordinator with the hook inbox installed is never typed at.
    let adopted = boop(
        &dir,
        &[
            "adopt",
            "--name",
            "ping-coord",
            "--tmux",
            &session.0,
            "--harness",
            "claude",
            "--cwd",
            dir.to_str().unwrap(),
            "--no-hooks",
        ],
    );
    assert!(adopted.status.success(), "stderr: {:?}", adopted.stderr);
    let hailed = boop(
        &dir,
        &[
            "hail",
            "--to",
            "ping-coord",
            "--from",
            "fake-lane",
            "--kind",
            "result",
            "--body",
            "lane fake-lane done rc=0",
        ],
    );
    assert!(hailed.status.success(), "stderr: {:?}", hailed.stderr);
    let stdout = String::from_utf8_lossy(&hailed.stdout);
    assert!(
        stdout.contains("injected into tmux"),
        "hail did not inject: {stdout}"
    );
    std::thread::sleep(std::time::Duration::from_millis(300));
    let captured = tmux(&["capture-pane", "-p", "-t", &session.0]);
    let pane = String::from_utf8_lossy(&captured.stdout);
    assert!(
        pane.contains("lane fake-lane done rc=0"),
        "pane never received the ping: {pane}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
