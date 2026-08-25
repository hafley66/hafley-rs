use std::path::{Path, PathBuf};
use std::process::Command;

const BOOP: &str = env!("CARGO_BIN_EXE_boop");

fn mail_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("boop-kinds-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(dir.join("home")).unwrap();
    dir
}

fn run(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(BOOP)
        .args(args)
        .arg("--mail-dir")
        .arg(dir)
        .env("HOME", dir.join("home"))
        .env("BOOP_DB", dir.join("boop.db"))
        .output()
        .unwrap()
}

/// A tmux session for as long as the value lives. `beep lane prune` refuses to
/// guess when no server answers at all, so a host with none running (a CI
/// runner) would otherwise decide the test below.
struct LiveTmuxSession(String);

impl LiveTmuxSession {
    fn new(name: &str) -> Self {
        let status = Command::new("tmux")
            .args(["new-session", "-d", "-s", name])
            .status()
            .expect("tmux installed");
        assert!(status.success(), "tmux must start {name}");
        LiveTmuxSession(name.to_owned())
    }
}

impl Drop for LiveTmuxSession {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &self.0])
            .status();
    }
}

#[test]
fn prune_skips_a_dead_coordinator_and_legacy_rows_still_prune() {
    let dir = mail_dir("prune");
    let _session = LiveTmuxSession::new(&format!("boop-kinds-live-{}", std::process::id()));
    std::fs::write(
        dir.join("registry.json"),
        r#"{
  "coord": {"kind":"coordinator", "tmux":"boop-no-coord"},
  "old": {"tmux":"boop-no-old"}
}"#,
    )
    .unwrap();
    let output = run(&dir, &["beep", "lane", "prune"]);
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    let routes = boop_store::testing::routes_json(&dir.join("boop.db"));
    assert!(routes.get("coord").is_some());
    assert!(routes.get("old").is_none());
}

#[test]
/// RECEIPT. `beep agent register --kind native` writes a route with no
/// harness, so nothing declares a door for it. The hail stays on the bus, the
/// ladder's last rung holds it, and the reason is named in stdout and in the
/// `agent_delivery` ledger.
fn a_hail_to_a_harnessless_native_row_is_held_and_the_reason_recorded() {
    let dir = mail_dir("hail");
    std::fs::write(
        dir.join("registry.json"),
        r#"{"native-worker":{"kind":"native"}}"#,
    )
    .unwrap();
    let output = run(&dir, &["beep", "hail", "native-worker", "--body", "hello"]);
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("route native-worker names no harness"),
        "stdout: {stdout}"
    );
    let mailbox =
        serde_json::Value::Array(boop_store::testing::mail_rows(&dir.join("boop.db"))).to_string();
    assert!(mailbox.contains("hello"));

    let ledger = Command::new(BOOP)
        .args([
            "db",
            "select outcome, detail from agent_delivery order by at_ms desc limit 1",
        ])
        .env("HOME", dir.join("home"))
        .env("BOOP_DB", dir.join("boop.db"))
        .output()
        .unwrap();
    assert!(ledger.status.success(), "stderr: {:?}", ledger.stderr);
    let rows = String::from_utf8_lossy(&ledger.stdout);
    assert!(rows.contains("held-in-mailbox"), "ledger: {rows}");
    assert!(
        rows.contains("route native-worker names no harness"),
        "ledger: {rows}"
    );
}

#[test]
fn agent_register_and_done_round_trip_registry_and_ledger() {
    let dir = mail_dir("agent");
    let registered = run(
        &dir,
        &[
            "beep",
            "agent",
            "register",
            "native-worker",
            "--kind",
            "native",
            "--parent",
            "coord",
        ],
    );
    assert!(
        registered.status.success(),
        "stderr: {:?}",
        registered.stderr
    );
    let routes = boop_store::testing::routes_json(&dir.join("boop.db"));
    assert_eq!(routes["native-worker"]["kind"], "native");
    assert_eq!(routes["native-worker"]["parent"], "coord");

    let done = run(
        &dir,
        &["beep", "agent", "done", "native-worker", "--rc", "7"],
    );
    assert!(done.status.success(), "stderr: {:?}", done.stderr);
    let routes = boop_store::testing::routes_json(&dir.join("boop.db"));
    assert!(routes.get("native-worker").is_none());
    let mailbox =
        serde_json::Value::Array(boop_store::testing::mail_rows(&dir.join("boop.db"))).to_string();
    assert!(mailbox.contains("lane native-worker done rc=7"));
    assert!(mailbox.contains("\"to\":\"coord\""));
}

/// The live receipt for the codex native cross-messaging chain: a lane whose
/// native subagent spawned a second lane whose own native answers it. Every
/// rung of that chain has to show as one nesting level, so a pane-less
/// coordinator or native is a node rather than a probe of a pane it never had.
#[test]
fn pstree_nests_a_native_between_the_two_lanes_it_joins() {
    let dir = mail_dir("pstree-native");
    std::fs::write(
        dir.join("registry.json"),
        r#"{
  "e2e-root": {"kind":"coordinator"},
  "feature-cx-a2": {"kind":"lane", "parent":"e2e-root", "tmux":"boop-no-cx-a2"},
  "native-n1b": {"kind":"native", "parent":"feature-cx-a2"},
  "feature-cx-b2": {"kind":"lane", "parent":"native-n1b", "tmux":"boop-no-cx-b2"},
  "native-n2b": {"kind":"native", "parent":"feature-cx-b2"}
}"#,
    )
    .unwrap();
    let output = run(&dir, &["beep", "pstree", "--all"]);
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    let text = String::from_utf8(output.stdout).unwrap();
    let depth = |name: &str| -> usize {
        let row = text
            .lines()
            .find(|line| line.trim_start().starts_with(name))
            .unwrap_or_else(|| panic!("{name} is missing from the tree:\n{text}"));
        row.len() - row.trim_start().len()
    };
    assert_eq!(depth("e2e-root"), 0, "{text}");
    assert!(depth("feature-cx-a2") > depth("e2e-root"), "{text}");
    assert!(depth("native-n1b") > depth("feature-cx-a2"), "{text}");
    assert!(depth("feature-cx-b2") > depth("native-n1b"), "{text}");
    assert!(depth("native-n2b") > depth("feature-cx-b2"), "{text}");
}

/// A pane-less agent has no pane to probe, so a live-only run keeps it too:
/// without it the tree breaks into one root per native.
#[test]
fn pstree_keeps_a_paneless_native_without_all() {
    let dir = mail_dir("pstree-live-native");
    std::fs::write(
        dir.join("registry.json"),
        r#"{
  "coord": {"kind":"coordinator"},
  "native-only": {"kind":"native", "parent":"coord"}
}"#,
    )
    .unwrap();
    let output = run(&dir, &["beep", "pstree"]);
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("native-only"), "{text}");
    assert!(!text.contains("[gone]"), "{text}");
}
