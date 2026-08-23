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
    let routes: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("registry.json")).unwrap()).unwrap();
    assert!(routes.get("coord").is_some());
    assert!(routes.get("old").is_none());
}

#[test]
/// RECEIPT. `beep agent register --kind native` writes a route with no
/// harness, so nothing declares a door for it. The hail stays on the bus and
/// the refusal is named, in stdout and in the `agent_delivery` ledger.
fn a_hail_to_a_harnessless_native_row_is_refused_by_name_and_recorded() {
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
    let mailbox = std::fs::read_to_string(dir.join("bus.ndjson")).unwrap();
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
    assert!(rows.contains("unreachable"), "ledger: {rows}");
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
    let routes: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("registry.json")).unwrap()).unwrap();
    assert_eq!(routes["native-worker"]["kind"], "native");
    assert_eq!(routes["native-worker"]["parent"], "coord");

    let done = run(
        &dir,
        &["beep", "agent", "done", "native-worker", "--rc", "7"],
    );
    assert!(done.status.success(), "stderr: {:?}", done.stderr);
    let routes: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("registry.json")).unwrap()).unwrap();
    assert!(routes.get("native-worker").is_none());
    let mailbox = std::fs::read_to_string(dir.join("bus.ndjson")).unwrap();
    assert!(mailbox.contains("lane native-worker done rc=7"));
    assert!(mailbox.contains("\"to\":\"coord\""));
}
