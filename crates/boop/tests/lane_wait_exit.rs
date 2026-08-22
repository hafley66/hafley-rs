//! The exit codes a spawn-and-join depends on, taken from the real binary:
//! `--wait` on `lane create` returns through this same verb.

use std::io::BufRead;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn mail_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("boop-wait-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(dir.join("home")).unwrap();
    dir
}

/// One result row in the shape the on-exit epilogue hails: from the lane, to
/// the parent that spawned it.
fn seed_result(dir: &std::path::Path, lane: &str, rc: i32) {
    let row = serde_json::json!({
        "id": "m-seed",
        "from": lane,
        "to": "sprefa-coordinator",
        "from_timestamp": "2026-08-10T00:00:00.000Z",
        "to_timestamp": null,
        "kind": "result",
        "reply_to": null,
        "body": format!("lane {lane} done rc={rc}"),
        "ref": null,
    });
    std::fs::write(dir.join("bus.ndjson"), format!("{row}\n")).unwrap();
}

fn seed_current_result(dir: &std::path::Path, lane: &str, rc: i32) {
    let row = serde_json::json!({
        "id": "m-create-result",
        "from": lane,
        "to": "sprefa-coordinator",
        "from_timestamp": boop::bus::now_iso(),
        "to_timestamp": null,
        "kind": "result",
        "reply_to": null,
        "body": format!("lane {lane} done rc={rc}"),
        "ref": null,
    });
    use std::io::Write;
    writeln!(
        std::fs::OpenOptions::new()
            .append(true)
            .open(dir.join("bus.ndjson"))
            .unwrap(),
        "{row}"
    )
    .unwrap();
}

fn executable(name: &str) -> PathBuf {
    let output = Command::new("sh")
        .args(["-c", &format!("command -v {name}")])
        .output()
        .unwrap();
    assert!(output.status.success(), "{name} is required by this test");
    PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
}

fn write_executable(path: &std::path::Path, body: &str) {
    std::fs::write(path, body).unwrap();
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).unwrap();
}

struct CreateFixture {
    root: PathBuf,
    repo: PathBuf,
    brief: PathBuf,
    mail: PathBuf,
    bin: PathBuf,
    socket: String,
    lane: String,
    tmux: String,
}

impl CreateFixture {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("boop-create-wait-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("repo");
        let mail = root.join("mail");
        let bin = root.join("bin");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&mail).unwrap();
        std::fs::create_dir_all(&bin).unwrap();

        let brief = repo.join("brief.md");
        std::fs::write(&brief, "finish and report\n").unwrap();
        Command::new(executable("git"))
            .args(["init", "-q"])
            .current_dir(&repo)
            .status()
            .unwrap();
        Command::new(executable("git"))
            .args(["add", "."])
            .current_dir(&repo)
            .status()
            .unwrap();
        Command::new(executable("git"))
            .args([
                "-c",
                "user.name=Boop Test",
                "-c",
                "user.email=boop@example.invalid",
                "commit",
                "-qm",
                "fixture",
            ])
            .current_dir(&repo)
            .status()
            .unwrap();

        symlink(executable("git"), bin.join("git")).unwrap();
        symlink(executable("tmux"), bin.join("tmux")).unwrap();
        let fake_boop = bin.join("boop");
        std::fs::write(&fake_boop, "#!/bin/sh\nexit 0\n").unwrap();
        let mut permissions = std::fs::metadata(&fake_boop).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&fake_boop, permissions).unwrap();

        Self {
            root,
            repo,
            brief,
            mail,
            bin,
            socket: format!("boop-create-wait-{}-{name}", std::process::id()),
            lane: format!("feature-create-wait-{name}"),
            tmux: format!("feature-create-wait-{name}"),
        }
    }

    fn command(&self, timeout: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_boop"));
        command
            .env_clear()
            .env("HOME", &self.root)
            .env("BOOP_DB", self.root.join("boop.db"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("PATH", &self.bin)
            .args(["beep", "lane", "create"])
            .arg("--lane")
            .arg(&self.lane)
            .arg("--cwd")
            .arg(&self.repo)
            .arg("--brief")
            .arg(&self.brief)
            .args(["--harness", "codex", "--model", "gpt-test"])
            .arg("--tmux")
            .arg(&self.tmux)
            .arg("--socket")
            .arg(&self.socket)
            .args(["--parent", "sprefa-coordinator"])
            .arg("--mail-dir")
            .arg(&self.mail)
            .args(["--no-start", "--wait", "--wait-timeout", timeout]);
        command
    }

    fn codex_caller_command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_boop"));
        command
            .env_clear()
            .env("HOME", &self.root)
            .env("BOOP_DB", self.root.join("boop.db"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("PATH", &self.bin)
            .env("CODEX_THREAD_ID", "thread-codex-parent")
            .env("TMUX_PANE", "%1206")
            .args(["beep", "lane", "create"])
            .arg("--lane")
            .arg(&self.lane)
            .arg("--cwd")
            .arg(&self.repo)
            .arg("--brief")
            .arg(&self.brief)
            .args(["--harness", "codex", "--model", "gpt-test"])
            .arg("--tmux")
            .arg(&self.tmux)
            .arg("--socket")
            .arg(&self.socket)
            .arg("--mail-dir")
            .arg(&self.mail)
            .args(["--no-start", "--wait", "--wait-timeout", "30", "--dry-run"]);
        command
    }
}

impl Drop for CreateFixture {
    fn drop(&mut self) {
        let _ = Command::new(executable("tmux"))
            .args(["-L", &self.socket, "kill-server"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn wait_exit(dir: &std::path::Path, lane: &str, timeout: &str) -> i32 {
    Command::new(env!("CARGO_BIN_EXE_boop"))
        .args(["beep", "lane", "wait", lane, "--timeout", timeout])
        .arg("--mail-dir")
        .arg(dir)
        .env("HOME", dir.join("home"))
        .env("BOOP_DB", dir.join("boop.db"))
        .status()
        .unwrap()
        .code()
        .unwrap()
}

/// RECEIPT. A lane's rc reaches the caller's shell unchanged, which is what
/// makes `lane create --wait` a spawn-and-join.
#[test]
fn a_wait_exits_with_the_lanes_own_rc() {
    let dir = mail_dir("rc");
    seed_result(&dir, "feature-schema-emit", 0);
    assert_eq!(wait_exit(&dir, "feature-schema-emit", "5"), 0);

    let dir = mail_dir("rc-fail");
    seed_result(&dir, "feature-schema-emit", 17);
    assert_eq!(wait_exit(&dir, "feature-schema-emit", "5"), 17);
    let _ = std::fs::remove_dir_all(&dir);
}

/// RECEIPT. A lane that never reports exits 124, the timeout code, so a
/// wedged lane cannot look like a success.
#[test]
fn a_wait_that_times_out_exits_124() {
    let dir = mail_dir("timeout");
    assert_eq!(wait_exit(&dir, "feature-never-lands", "1"), 124);
    let _ = std::fs::remove_dir_all(&dir);
}

/// RECEIPT. `create --wait` prints its route before it blocks, then returns
/// the rc from the result row written for that spawn.
#[test]
fn create_wait_returns_the_lanes_rc() {
    let fixture = CreateFixture::new("rc");
    let mut child = fixture
        .command("10")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut lines = std::io::BufReader::new(stdout).lines();
    let route = lines.next().unwrap().unwrap();
    let (_, route) = route.split_once(" -> ").unwrap();
    assert_eq!(
        route,
        format!("{} (tmux {})", fixture.lane, fixture.tmux),
        "the route line must be flushed before the result row exists"
    );
    seed_current_result(&fixture.mail, &fixture.lane, 17);
    assert_eq!(child.wait().unwrap().code(), Some(17));
}

/// RECEIPT. `create --wait --wait-timeout 1` uses the standalone waiter's exit
/// contract when no result row lands.
#[test]
fn create_wait_timeout_exits_124() {
    let fixture = CreateFixture::new("timeout");
    assert_eq!(
        fixture
            .command("1")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .code(),
        Some(124)
    );
}

/// RECEIPT. Dry-run reports the literal spawn and wait settings, then exits
/// without opening the tmux session.
#[test]
fn create_dry_run_with_wait_does_not_block() {
    let fixture = CreateFixture::new("dry-run");
    let output = fixture.command("30").arg("--dry-run").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("cmd: "), "{stdout}");
    assert!(
        stdout.contains(&format!("tmux: {}", fixture.tmux)),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("wait: for {} result, timeout 30s", fixture.lane)),
        "{stdout}"
    );
    assert!(!Command::new(executable("tmux"))
        .args(["-L", &fixture.socket, "has-session", "-t", &fixture.tmux])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success());
}

/// RECEIPT. A fresh Codex tool subprocess has a runtime thread id and an
/// inherited pane before Boop has a registry row. `lane create --wait` must
/// first persist that evidence as a coordinator route, then derive the
/// completion parent from it without requiring `boop me`.
#[test]
fn a_fresh_codex_caller_registers_and_parents_a_waiting_lane() {
    let fixture = CreateFixture::new("fresh-codex-parent");
    let output = fixture.codex_caller_command().output().unwrap();
    assert!(
        output.status.success(),
        "stdout={}\\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("parent: codex-1206 (from caller; completion hail appended on exit)"),
        "{stdout}"
    );

    let registry: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(fixture.mail.join("registry.json")).unwrap())
            .unwrap();
    assert_eq!(
        registry["codex-1206"],
        serde_json::json!({
            "kind": "coordinator",
            "harness": "codex",
            "tmux": "%1206",
            "cwd": fixture.repo.display().to_string(),
            "mode": "interactive",
            "sessionId": "thread-codex-parent",
            "registeredAt": registry["codex-1206"]["registeredAt"],
        })
    );
}

/// RECEIPT. A codex ACP session id arrives before the first turn. The lane
/// must still receive the complete brief; a session id alone carries no
/// evidence that any prior turn saw it. The fake shadows `npx`, which is what
/// `CODEX_ADAPTER` spawns.
#[test]
fn a_fresh_codex_acp_session_receives_the_lane_brief() {
    let root = mail_dir("fresh-codex-brief");
    let repo = root.join("repo");
    let mail = root.join("mail");
    let bin = root.join("bin");
    let log = root.join("codex-rpc.ndjson");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::create_dir_all(&mail).unwrap();
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(
        mail.join("registry.json"),
        r#"{"feature-fresh-codex":{"kind":"lane","parent":"coordinator"}}"#,
    )
    .unwrap();
    let brief = repo.join("brief.md");
    let brief_text = "FRESH_CODEX_BRIEF_SENTINEL execute the complete lane task\n";
    std::fs::write(&brief, brief_text).unwrap();

    write_executable(
        &bin.join("npx"),
        r#"#!/bin/sh
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$BOOP_TEST_CODEX_LOG"
  id=$(printf '%s\n' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  case "$line" in
    *'"method":"initialize"'*)
      printf '{"jsonrpc":"2.0","id":"%s","result":{"protocolVersion":1,"agentCapabilities":{},"authMethods":[]}}\n' "$id"
      ;;
    *'"method":"session/new"'*)
      printf '{"jsonrpc":"2.0","id":"%s","result":{"sessionId":"fresh-acp-session"}}\n' "$id"
      ;;
    *'"method":"session/set_config_option"'*)
      printf '{"jsonrpc":"2.0","id":"%s","result":{"configOptions":[]}}\n' "$id"
      ;;
    *'"method":"session/prompt"'*)
      printf '{"jsonrpc":"2.0","id":"%s","result":{"stopReason":"end_turn"}}\n' "$id"
      ;;
  esac
done
"#,
    );

    let path = format!("{}:/usr/bin:/bin", bin.display());
    let mut child = Command::new(env!("CARGO_BIN_EXE_boop"))
        .env("PATH", path)
        .env("HOME", root.join("home"))
        .env("BOOP_DB", root.join("boop.db"))
        .env("BOOP_TEST_CODEX_LOG", &log)
        .current_dir(&repo)
        .args([
            "beep",
            "lane",
            "run",
            "--lane",
            "feature-fresh-codex",
            "--harness",
            "codex",
            "--brief",
        ])
        .arg(&brief)
        .args(["--model", "gpt-test", "--mail-dir"])
        .arg(&mail)
        .spawn()
        .unwrap();

    // A lane whose turn completes with no pending mail parks instead of
    // exiting, so this waits for the result row rather than for the process.
    let bus = mail.join("bus.ndjson");
    let start = std::time::Instant::now();
    while !std::fs::read_to_string(&bus)
        .unwrap_or_default()
        .contains("lane feature-fresh-codex done rc=0")
    {
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "no result row within the wait budget"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();

    let rpc = std::fs::read_to_string(&log).unwrap();
    assert!(rpc.contains("FRESH_CODEX_BRIEF_SENTINEL"), "{rpc}");
    assert!(
        !rpc.contains("previous turn ended on a provider error"),
        "{rpc}"
    );
    let result = std::fs::read_to_string(mail.join("bus.ndjson")).unwrap();
    assert!(
        result.contains("lane feature-fresh-codex done rc=0"),
        "{result}"
    );

    let _ = std::fs::remove_dir_all(&root);
}
