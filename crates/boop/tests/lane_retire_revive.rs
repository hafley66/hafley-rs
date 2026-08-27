//! E2E, real binary, real tmux, fake codex ACP agent on PATH. A lane that has
//! mailed its result row and sees no mail for BOOP_IDLE_SHUTDOWN_SECS closes
//! its harness and leaves its pane; a later `boop beep <lane> <body>` replays
//! the spawn record, resumes the pinned conversation through `session/load`,
//! hands the body over as the opening turn, and the sender's wait ends on the
//! lane's turn-end row.
//!
//! FAIL-PRE-FIX: 17 finished lanes sat parked for three days holding a
//! 130-165 MB harness child each (2026-08-27), and nothing could bring a
//! stopped lane back on its conversation.

use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn executable(name: &str) -> PathBuf {
    let output = Command::new("sh")
        .args(["-c", &format!("command -v {name}")])
        .output()
        .unwrap();
    assert!(output.status.success(), "{name} is required by this test");
    PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
}

fn write_executable(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).unwrap();
}

/// The ACP agent the codex adapter spawns as `npx`: advertises loadSession,
/// answers every prompt with end_turn, and logs each request line.
const FAKE_NPX: &str = r#"#!/bin/sh
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$BOOP_TEST_CODEX_LOG"
  id=$(printf '%s\n' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  case "$line" in
    *'"method":"initialize"'*)
      printf '{"jsonrpc":"2.0","id":"%s","result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true},"authMethods":[]}}\n' "$id"
      ;;
    *'"method":"session/new"'*)
      printf '{"jsonrpc":"2.0","id":"%s","result":{"sessionId":"retire-acp-session"}}\n' "$id"
      ;;
    *'"method":"session/load"'*)
      printf '{"jsonrpc":"2.0","id":"%s","result":{"configOptions":[]}}\n' "$id"
      ;;
    *'"method":"session/set_config_option"'*)
      printf '{"jsonrpc":"2.0","id":"%s","result":{"configOptions":[]}}\n' "$id"
      ;;
    *'"method":"session/prompt"'*)
      printf '{"jsonrpc":"2.0","id":"%s","result":{"stopReason":"end_turn"}}\n' "$id"
      ;;
  esac
done
"#;

struct Fixture {
    root: PathBuf,
    repo: PathBuf,
    brief: PathBuf,
    mail: PathBuf,
    bin: PathBuf,
    log: PathBuf,
    socket: String,
    lane: String,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("boop-retire-revive-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("repo");
        let mail = root.join("mail");
        let bin = root.join("bin");
        for dir in [&repo, &mail, &bin, &root.join("home")] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let brief = repo.join("brief.md");
        std::fs::write(&brief, "RETIRE_BRIEF_SENTINEL finish and report\n").unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["add", "."],
            vec![
                "-c",
                "user.name=Boop Test",
                "-c",
                "user.email=boop@example.invalid",
                "commit",
                "-qm",
                "fixture",
            ],
        ] {
            Command::new(executable("git"))
                .args(args)
                .current_dir(&repo)
                .status()
                .unwrap();
        }
        symlink(executable("git"), bin.join("git")).unwrap();
        symlink(executable("tmux"), bin.join("tmux")).unwrap();
        symlink(executable("nice"), bin.join("nice")).unwrap();
        symlink(executable("sed"), bin.join("sed")).unwrap();
        symlink(executable("sh"), bin.join("sh")).unwrap();
        // The pane runs the real supervisor from this build.
        symlink(env!("CARGO_BIN_EXE_boop"), bin.join("boop")).unwrap();
        write_executable(&bin.join("npx"), FAKE_NPX);
        std::fs::write(
            mail.join("registry.json"),
            serde_json::json!({ "sprefa-coordinator": { "kind": "coordinator" } }).to_string(),
        )
        .unwrap();
        Self {
            log: root.join("codex-rpc.ndjson"),
            socket: format!("boop-retire-{}", std::process::id()),
            lane: "feature-retire".to_owned(),
            root,
            repo,
            brief,
            mail,
            bin,
        }
    }

    fn boop(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_boop"));
        command
            .env_clear()
            .env("HOME", self.root.join("home"))
            .env("BOOP_DB", self.root.join("boop.db"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("PATH", format!("{}:/usr/bin:/bin", self.bin.display()))
            .env("BOOP_TEST_CODEX_LOG", &self.log)
            .env("BOOP_IDLE_SHUTDOWN_SECS", "1")
            .env("BOOP_NO_SYNC", "1")
            .current_dir(&self.repo);
        command
    }

    fn pane_alive(&self) -> bool {
        Command::new(executable("tmux"))
            .args(["-L", &self.socket, "has-session", "-t", &self.lane])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn mailbox(&self) -> String {
        serde_json::Value::Array(boop_store::testing::mail_rows(&self.mail.join("boop.db")))
            .to_string()
    }

    fn rpc_log(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn supervise_log(&self) -> String {
        std::fs::read_to_string(self.trail("supervise.log")).unwrap_or_default()
    }

    fn trail(&self, file: &str) -> PathBuf {
        self.root
            .join("home")
            .join(".agent")
            .join("lanes")
            .join(&self.lane)
            .join(file)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = Command::new(executable("tmux"))
            .args(["-L", &self.socket, "kill-server"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn wait_for(what: &str, mut ready: impl FnMut() -> bool, timeout: Duration) {
    let start = Instant::now();
    while !ready() {
        assert!(
            start.elapsed() < timeout,
            "{what} did not happen within {timeout:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_finished_lane_retires_and_a_beep_revives_it_on_the_same_conversation() {
    let fx = Fixture::new();

    // 1. spawn through the real verb; the pane runs the real supervisor.
    let create = fx
        .boop()
        .args([
            "beep", "lane", "create", "--lane", &fx.lane, "--tmux", &fx.lane,
        ])
        .arg("--cwd")
        .arg(&fx.repo)
        .arg("--brief")
        .arg(&fx.brief)
        .args([
            "--harness",
            "codex",
            "--model",
            "gpt-test",
            "--socket",
            &fx.socket,
        ])
        .args(["--parent", "sprefa-coordinator", "--no-start", "--mail-dir"])
        .arg(&fx.mail)
        .output()
        .unwrap();
    assert!(
        create.status.success(),
        "lane create failed:\n{}{}",
        String::from_utf8_lossy(&create.stdout),
        String::from_utf8_lossy(&create.stderr)
    );

    // 2. the brief turn completes and the result row lands.
    wait_for(
        "result row",
        || fx.mailbox().contains("lane feature-retire done rc=0"),
        Duration::from_secs(20),
    );
    // 3. one second of silence: the lane retires, its pane is gone, the
    //    parent holds one note, and the trail keeps what a revive needs.
    wait_for(
        "retire note",
        || fx.mailbox().contains("lane feature-retire retired"),
        Duration::from_secs(20),
    );
    wait_for("pane exit", || !fx.pane_alive(), Duration::from_secs(20));
    assert!(fx.trail("spawn.json").exists(), "spawn record missing");
    assert_eq!(
        std::fs::read_to_string(fx.trail("conversation"))
            .unwrap()
            .trim(),
        "retire-acp-session"
    );
    let log_before = fx.rpc_log();
    assert_eq!(log_before.matches("\"method\":\"session/new\"").count(), 1);
    assert_eq!(log_before.matches("RETIRE_BRIEF_SENTINEL").count(), 1);
    assert_eq!(
        fx.mailbox().matches("lane feature-retire done rc=").count(),
        1,
        "retirement must not write a second result row"
    );

    // 3b. the registry lost the route, and `lane list` still shows the lane.
    let list = fx
        .boop()
        .args(["beep", "lane", "list", "--state", "retired", "--mail-dir"])
        .arg(&fx.mail)
        .output()
        .unwrap();
    let list = String::from_utf8_lossy(&list.stdout);
    assert!(
        list.contains("retired") && list.contains("feature-retire") && list.contains("REVIVE="),
        "lane list must show the retired lane with its revive spelling:\n{list}"
    );

    // 4. a send to the retired lane revives it and returns on its turn end.
    let beep = fx
        .boop()
        .args(["beep", &fx.lane, "SECOND_TASK_SENTINEL do one more thing"])
        .args([
            "--as",
            "sprefa-coordinator",
            "--timeout",
            "60",
            "--mail-dir",
        ])
        .arg(&fx.mail)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&beep.stdout);
    let stderr = String::from_utf8_lossy(&beep.stderr);
    assert!(
        beep.status.success(),
        "beep to the retired lane failed:\n{stdout}{stderr}\n--- supervise.log\n{}",
        fx.supervise_log()
    );
    assert!(
        stdout.contains("revive feature-retire"),
        "no revive line:\n{stdout}"
    );
    assert!(
        stdout.contains("revived feature-retire"),
        "no revived line:\n{stdout}"
    );
    assert!(
        stdout.contains("idle feature-retire turn="),
        "the wait must end on the lane's turn-end row:\n{stdout}"
    );

    // 5. the same conversation, resumed, and the body was the opening turn:
    //    no second session/new, the brief was never re-fed.
    let log_after = fx.rpc_log();
    assert_eq!(log_after.matches("\"method\":\"session/new\"").count(), 1);
    assert!(
        log_after.contains("\"method\":\"session/load\"")
            && log_after.contains("retire-acp-session"),
        "revive must resume through session/load:\n{log_after}"
    );
    assert_eq!(log_after.matches("RETIRE_BRIEF_SENTINEL").count(), 1);
    assert!(
        log_after.contains("SECOND_TASK_SENTINEL"),
        "body never reached the harness"
    );

    // 6. and it retires again on its own.
    wait_for(
        "second retirement",
        || !fx.pane_alive(),
        Duration::from_secs(20),
    );
    wait_for(
        "second retire note",
        || fx.mailbox().matches("lane feature-retire retired").count() == 2,
        Duration::from_secs(20),
    );
    assert_eq!(
        fx.mailbox().matches("lane feature-retire retired").count(),
        2
    );
}
