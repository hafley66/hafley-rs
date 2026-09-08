//! End-to-end coordinator ping delivery over a real tmux session.
//!
//! FAIL-PRE-FIX: registering a coordinator route wrote `kind: "lane"`, so a
//! hail took the "lane supervisor delivers it" branch and every result row
//! addressed to a coordinator queued in bus.ndjson forever, never reaching
//! the pane.
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
    boop_store::testing::routes_json(&dir.join("boop.db"))[name]["kind"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

/// Seed a coordinator route for the deterministic door tests.
fn write_coordinator_route(dir: &Path, name: &str, tmux: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("registry.json"),
        serde_json::json!({
            name: {
                "kind": "coordinator",
                "harness": "claude",
                "tmux": tmux,
                "cwd": dir.to_str().unwrap(),
            }
        })
        .to_string(),
    )
    .unwrap();
}

#[test]
fn lane_patch_preserves_an_existing_lane_route() {
    let dir = mail_dir("patch");
    let session = TestSession::new("patch");
    std::fs::write(dir.join("registry.json"), r#"{"test-lane":{"kind":"lane"}}"#).unwrap();
    let output = boop(
        &dir,
        &["beep", "lane", "patch", "test-lane", "--tmux", &session.0],
    );
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    assert_eq!(route_kind(&dir, "test-lane"), "lane");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn lane_patch_of_a_fresh_pane_registers_a_coordinator_without_a_supervisor() {
    let dir = mail_dir("fresh-patch");
    let session = TestSession::new("fresh-patch");
    let output = boop(&dir, &["beep", "lane", "patch", "parent", "--tmux", &session.0]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(route_kind(&dir, "parent"), "coordinator");
}

/// RECEIPT. A hail to a claude coordinator route goes to the claude door.
/// With no claude session live in that pane and no hook inbox in the project,
/// nothing takes the row: the refusal is named on stdout, written to the
/// `agent_delivery` ledger, and the pane is never typed at.
#[test]
fn hail_to_a_coordinator_with_no_live_session_is_held_for_its_turn_boundary() {
    let dir = mail_dir("deliver");
    let session = TestSession::new("deliver");
    write_coordinator_route(&dir, "ping-coord", &session.0);
    let hailed = boop(
        &dir,
        &[
            "beep",
            "ping-coord",
            "answer me when you can",
            "--as",
            "fake-lane",
            "--kind",
            "request",
            "--no-wait",
        ],
    );
    assert!(hailed.status.success(), "stderr: {:?}", hailed.stderr);
    let stdout = String::from_utf8_lossy(&hailed.stdout);
    assert!(
        stdout.contains("no live claude session for ping-coord"),
        "the landing line must name the door it tried: {stdout}"
    );
    assert!(
        stdout.contains("held") && stdout.contains("turn boundary"),
        "a claude route whose door is down is held, never pasted at: {stdout}"
    );

    let ledger = Command::new(BOOP)
        .args([
            "db",
            "select d.route, h.value as harness, d.outcome, d.detail from agent_delivery d \
             left join dict_harness h on h.id = d.harness_id order by d.at_ms desc limit 1",
        ])
        .env("BOOP_DB", dir.join("boop.db"))
        .env("HOME", dir.join("home"))
        .output()
        .unwrap();
    assert!(ledger.status.success(), "stderr: {:?}", ledger.stderr);
    let row = String::from_utf8_lossy(&ledger.stdout);
    assert!(row.contains("ping-coord"), "ledger: {row}");
    assert!(row.contains("claude"), "ledger: {row}");
    assert!(row.contains("held-for-turn-boundary"), "ledger: {row}");

    std::thread::sleep(std::time::Duration::from_millis(300));
    let captured = tmux(&["capture-pane", "-p", "-t", &session.0]);
    let pane = String::from_utf8_lossy(&captured.stdout);
    assert!(
        !pane.contains("answer me when you can"),
        "the pane received keystrokes: {pane}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// RECEIPT. The same coordinator route, one kind lower: a lane's `result` row
/// is an end row (Chris, 2026-09-07), so it walks the ladder like a hail and,
/// with no live claude session behind the route, is held for the
/// coordinator's next turn boundary. A `yield` row still stops at the mailbox
/// (supervisor-rows-off-the-door).
#[test]
fn a_supervisor_result_row_to_the_same_coordinator_stops_at_the_mailbox() {
    let dir = mail_dir("supervisor");
    let session = TestSession::new("supervisor");
    write_coordinator_route(&dir, "ping-coord", &session.0);
    let hailed = boop(
        &dir,
        &[
            "beep",
            "ping-coord",
            "lane fake-lane done rc=0",
            "--as",
            "fake-lane",
            "--kind",
            "result",
            "--no-wait",
        ],
    );
    assert!(hailed.status.success(), "stderr: {:?}", hailed.stderr);
    let stdout = String::from_utf8_lossy(&hailed.stdout);
    assert!(
        stdout.contains("for the next turn boundary"),
        "an end row walks the ladder like a hail: {stdout}"
    );

    let progress = boop(
        &dir,
        &[
            "beep",
            "ping-coord",
            "idle fake-lane turn=1 head=abc dirty=0",
            "--as",
            "fake-lane",
            "--kind",
            "yield",
            "--no-wait",
        ],
    );
    assert!(progress.status.success(), "stderr: {:?}", progress.stderr);
    let stdout = String::from_utf8_lossy(&progress.stdout);
    assert!(
        stdout.contains("in the mailbox (yield row; no door)"),
        "a progress row names the kind that skipped the door: {stdout}"
    );
    assert!(
        stdout.contains("reads it with `boop wait`"),
        "the landing line names the verb that collects it: {stdout}"
    );

    let ledger = Command::new(BOOP)
        .args([
            "db",
            "select d.outcome, d.detail from agent_delivery d where d.detail like 'yield row%' order by d.at_ms desc limit 1",
        ])
        .env("BOOP_DB", dir.join("boop.db"))
        .env("HOME", dir.join("home"))
        .output()
        .unwrap();
    let row = String::from_utf8_lossy(&ledger.stdout);
    assert!(row.contains("held-in-mailbox"), "ledger: {row}");
    assert!(
        !row.contains("held-for-turn-boundary"),
        "a yield row never reaches the turn-boundary rung: {row}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
