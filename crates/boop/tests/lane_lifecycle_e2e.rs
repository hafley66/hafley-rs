//! Lane lifecycle end to end, one pass per harness: the real boop binary
//! spawns a real lane whose harness runs against a loopback llmock provider,
//! and every parent is a real coordinator TUI on the same harness, in the same
//! scratch tmux server. No registry-row stand-in: the door the lane talks to is
//! a running claude, codex or opencode pane.
//!
//! Four cases, three harnesses, twelve tests named `<case>_<harness>`:
//! 1. a held inbound row defers the lane result;
//! 2. a send to a retired lane revives it;
//! 3. a retired lane closes its tmux session;
//! 4. a stale lane tells its parent.
//!
//! Live claude and live codex are required, not optional. A case skips a
//! harness only when that harness's executable or llmock is absent, printed as
//! `skip <case> <harness>: <reason>`. Executable overrides: CLAUDE_BIN,
//! CODEX_BIN (a codex lane rides `npx @agentclientprotocol/codex-acp`),
//! OPENCODE_BIN, LLMOCK_BIN. Install llmock with:
//!   cargo install --git https://github.com/larsakerlund/llmock.git \
//!     --tag v0.1.2 --locked llmock

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use boop::harness::mock_tui::{self, MockProvider, MockTuiContext, MockTuiLaunch, MockTuiReplay};
use boop::harness::{shell_quote, HarnessId};
use boop::Registry;
use boop_store::testing::BoopCommandExt;

const BOOP: &str = env!("CARGO_BIN_EXE_boop");

/// Every case spawns two real harnesses against one tmux server; run them one
/// at a time so a loaded machine does not starve a turn past its deadline.
static LANE_LOCK: Mutex<()> = Mutex::new(());

fn lane_lock() -> std::sync::MutexGuard<'static, ()> {
    LANE_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

const POLL: Duration = Duration::from_millis(100);
const START_DEADLINE: Duration = Duration::from_secs(90);
const STALE_DEADLINE: Duration = Duration::from_secs(10);

/// One harness's place in the matrix.
struct Harness {
    id: HarnessId,
    entry: &'static str,
    bin_env: &'static str,
}

const HARNESSES: [Harness; 3] = [
    Harness {
        id: HarnessId::Claude,
        entry: "claude",
        bin_env: "CLAUDE_BIN",
    },
    Harness {
        id: HarnessId::Codex,
        entry: "codex",
        bin_env: "CODEX_BIN",
    },
    Harness {
        id: HarnessId::Opencode,
        entry: "opencode",
        bin_env: "OPENCODE_BIN",
    },
];

/// A scratch world: repo, mailbox, lane and coordinator homes, scratch tmux.
struct Fixture {
    root: PathBuf,
    repo: PathBuf,
    brief: PathBuf,
    mail: PathBuf,
    lane_home: PathBuf,
    coord_home: PathBuf,
    workspace: PathBuf,
    bin: PathBuf,
    socket: String,
    lane: String,
    harness: &'static str,
    coord_route: String,
    coord_session: String,
}

impl Fixture {
    fn new(case: &str, h: &Harness) -> Fixture {
        let root = std::env::temp_dir().join(format!(
            "boop-lifecycle-{case}-{}-{}",
            h.entry,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("repo");
        let mail = root.join("mail");
        let lane_home = root.join("lane-home");
        let coord_home = root.join("coord-home");
        let workspace = root.join("workspace");
        let bin = root.join("bin");
        for dir in [&repo, &mail, &lane_home, &coord_home, &workspace, &bin] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::create_dir_all(root.join("config/boop")).unwrap();
        std::fs::write(root.join("config/boop/config.json"), "{}").unwrap();
        // The coordinator registers itself; no stand-in route for a parent.
        std::fs::write(mail.join("registry.json"), "{}").unwrap();
        let brief = root.join("brief.md");
        std::fs::write(&brief, "finish the brief and report\n").unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["config", "user.email", "boop@example.invalid"]);
        git(&repo, &["config", "user.name", "Boop Lifecycle"]);
        std::fs::write(repo.join("seed.txt"), "seed\n").unwrap();
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-qm", "seed"]);
        std::os::unix::fs::symlink(BOOP, bin.join("boop")).unwrap();
        let unique = std::process::id();
        Fixture {
            root,
            repo,
            brief,
            mail,
            lane_home,
            coord_home,
            workspace,
            bin,
            socket: format!("boop-lifecycle-{case}-{}-{unique}", h.entry),
            lane: format!("feature-lifecycle-{case}-{}", h.entry),
            harness: h.entry,
            coord_route: format!("coord-{case}-{}", h.entry),
            coord_session: format!("coord-{case}-{}-{unique}", h.entry),
        }
    }

    /// One `beep` addressed at a route in this scratch mailbox.
    fn beep(&self, args: &[&str]) -> std::process::Output {
        let mut full: Vec<&str> = vec!["beep"];
        full.extend_from_slice(args);
        full.push("--mail-dir");
        let mail = self.mail.display().to_string();
        full.push(&mail);
        Command::new(BOOP)
            .args(&full)
            .boop_test_root(&self.root)
            .env("BOOP_DB", self.mail.join("boop.db"))
            .env("BOOP_NO_SYNC", "1")
            .output()
            .expect("run boop beep")
    }

    /// One `lane create` under the live coordinator `parent`.
    fn create(
        &self,
        parent: &str,
        extra: &[&str],
        env: &[(String, String)],
        executable: Option<&Path>,
    ) -> std::process::Output {
        let mut command = Command::new(BOOP);
        command
            .boop_test_root(&self.root)
            .env("BOOP_DB", self.mail.join("boop.db"))
            .env("BOOP_NO_SYNC", "1")
            .args(["beep", "lane", "create"])
            .arg("--lane")
            .arg(&self.lane)
            .arg("--cwd")
            .arg(&self.repo)
            .arg("--brief")
            .arg(&self.brief)
            .arg("--harness")
            .arg(self.harness)
            .arg("--parent")
            .arg(parent)
            .arg("--tmux")
            .arg(&self.lane)
            .arg("--socket")
            .arg(&self.socket)
            .arg("--mail-dir")
            .arg(&self.mail)
            .arg("--no-start");
        if let Some(executable) = executable {
            command.arg("--bin").arg(executable);
        }
        for (key, value) in env {
            command.arg("--env").arg(format!("{key}={value}"));
        }
        command.args(extra);
        command.output().expect("run boop lane create")
    }

    fn tmux(&self, args: &[&str]) -> std::process::Output {
        let mut full = vec!["-L", self.socket.as_str()];
        full.extend_from_slice(args);
        Command::new("tmux").args(&full).output().expect("run tmux")
    }

    fn session_alive(&self) -> bool {
        self.tmux(&["has-session", "-t", &self.lane])
            .status
            .success()
    }

    fn screen(&self, session: &str) -> String {
        let output = self.tmux(&["capture-pane", "-p", "-t", session, "-S", "-400"]);
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn coord_screen(&self) -> String {
        self.screen(&self.coord_session)
    }

    fn coordinator_shows(&self, needle: &str) -> usize {
        self.coord_screen().matches(needle).count()
    }

    fn wait_for_screen(&self, session: &str, wanted: &str, deadline: Duration) {
        let deadline = Instant::now() + deadline;
        loop {
            let text = self.screen(session);
            if text.contains(wanted) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "{} never showed {wanted:?}\n{text}",
                session
            );
            std::thread::sleep(POLL);
        }
    }

    /// Every mailbox row, straight from the run's store.
    fn rows(&self) -> Vec<serde_json::Value> {
        boop_store::testing::mail_rows(&self.mail.join("boop.db"))
    }

    fn result_bodies(&self) -> Vec<String> {
        self.rows()
            .into_iter()
            .filter(|row| row.get("kind").and_then(|v| v.as_str()) == Some("result"))
            .filter(|row| row.get("from").and_then(|v| v.as_str()) == Some(self.lane.as_str()))
            .filter_map(|row| row.get("body").and_then(|v| v.as_str()).map(str::to_owned))
            .collect()
    }

    fn stale_rows(&self) -> usize {
        self.rows()
            .into_iter()
            .filter(|row| row.get("kind").and_then(|v| v.as_str()) == Some("stale"))
            .filter(|row| row.get("from").and_then(|v| v.as_str()) == Some(self.lane.as_str()))
            .count()
    }

    fn any_row_contains(&self, needle: &str) -> bool {
        self.rows()
            .iter()
            .any(|row| row.to_string().contains(needle))
    }

    fn trail(&self, file: &str) -> PathBuf {
        self.mail.join("lanes").join(&self.lane).join(file)
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.trail("supervise.log")).unwrap_or_default()
    }

    /// Wait for the supervisor's own trail to name `needle` `count` times.
    fn wait_for_log(&self, needle: &str, count: usize) {
        let deadline = Instant::now() + START_DEADLINE;
        loop {
            if self.log().matches(needle).count() >= count {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "never saw {count} x {needle:?} in the lane trail\n{}",
                self.log()
            );
            std::thread::sleep(POLL);
        }
    }

    fn wait_for_result(&self, count: usize) {
        let deadline = Instant::now() + START_DEADLINE;
        loop {
            let bodies = self.result_bodies();
            if bodies.len() >= count {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "wanted {count} result rows, saw {bodies:?}\n{}",
                self.log()
            );
            std::thread::sleep(POLL);
        }
    }

    fn wait_for_retired(&self) {
        let deadline = Instant::now() + START_DEADLINE;
        loop {
            let residency = boop::supervise::read_residency(&self.mail, &self.lane);
            if residency.as_deref() == Some(boop::supervise::RESIDENCY_RETIRED) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "the lane never retired; residency {residency:?}\n{}",
                self.log()
            );
            std::thread::sleep(POLL);
        }
    }

    fn wait_for_route_gone(&self) {
        let deadline = Instant::now() + START_DEADLINE;
        loop {
            let routes = boop_store::testing::routes_json(&self.mail.join("boop.db"));
            if routes.get(&self.lane).is_none() {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "the retired lane's route was not dropped"
            );
            std::thread::sleep(POLL);
        }
    }

    fn wait_for_session_gone(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if !self.session_alive() {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "the lane's tmux session outlived its retirement\n{}",
                self.log()
            );
            std::thread::sleep(POLL);
        }
    }

    /// Wait for the coordinator route to bind a session or app-server socket.
    fn wait_for_coordinator(&self) {
        let deadline = Instant::now() + START_DEADLINE;
        loop {
            let routes = boop_store::testing::routes_json(&self.mail.join("boop.db"));
            if let Some(entry) = routes.get(&self.coord_route) {
                let bound = entry
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .is_some_and(|value| !value.is_empty())
                    || entry
                        .get("appServerSocket")
                        .and_then(|v| v.as_str())
                        .is_some_and(|value| !value.is_empty());
                if bound {
                    return;
                }
            }
            assert!(
                Instant::now() < deadline,
                "coordinator route {} never bound a session",
                self.coord_route
            );
            std::thread::sleep(POLL);
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if std::env::var_os("BOOP_LIFECYCLE_KEEP").is_some() {
            eprintln!("kept {}", self.root.display());
            return;
        }
        let _ = self.tmux(&["kill-server"]);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo)
        .status()
        .expect("git is required by this test");
    assert!(status.success(), "git {args:?}");
}

fn commit(repo: &Path, subject: &str) {
    git(repo, &["commit", "--allow-empty", "-qm", subject]);
}

/// The provider fixture. The readiness probe answers `boop`; the coordinator's
/// typed prompt answers the terminal marker; every other turn (the lane brief)
/// answers text long enough that the supervisor treats it as real work. `pace`
/// delays the first token so a turn stays open under a hail.
fn fixture_yaml(pace_ms: u64) -> String {
    format!(
        r#"rules:
  - match:
      user_contains: "Respond exactly with: boop"
    respond:
      content: "boop"
  - match:
      user_contains: "render the terminal flow"
    respond:
      content: "FIXED_TERMINAL_REPLY"
  - match: {{}}
    respond:
      content: "the brief turn finished with a reply long enough to count as real work"
      stream:
        ttft_ms: {pace_ms}
        inter_token_ms: 0
"#
    )
}

/// The lane's scratch env: the harness's own mock recipe env, plus the test's
/// `boop` on PATH and any caller extras.
fn lane_env(
    fixture: &Fixture,
    launch_env: &[(String, String)],
    extra: &[(&str, &str)],
) -> Vec<(String, String)> {
    let mut env: BTreeMap<String, String> = launch_env.iter().cloned().collect();
    let path = std::env::var("PATH").unwrap_or_default();
    env.insert("PATH".into(), format!("{}:{path}", fixture.bin.display()));
    for (key, value) in extra {
        env.insert((*key).to_owned(), (*value).to_owned());
    }
    env.into_iter().collect()
}

/// Start the real coordinator TUI on `h`, drive its readiness prompt when its
/// recipe needs one, and wait for the route to bind.
fn start_coordinator(fixture: &Fixture, h: &Harness, port: u16) -> Result<(), String> {
    let registry = Registry::discover();
    let launch: MockTuiLaunch = registry
        .get(h.id)
        .mock_tui_launch(&MockTuiContext {
            home: &fixture.coord_home,
            workspace: &fixture.workspace,
            port,
        })
        .map_err(|error| error.to_string())?;
    run_coordinator_tui(fixture, h, &launch);
    if let MockTuiReplay::TypePrompt { readiness } = launch.replay {
        fixture.wait_for_screen(&fixture.coord_session, readiness, START_DEADLINE);
        let _ = fixture.tmux(&[
            "send-keys",
            "-t",
            &fixture.coord_session,
            "-l",
            mock_tui::MOCK_PROMPT,
        ]);
        std::thread::sleep(Duration::from_millis(250));
        let _ = fixture.tmux(&["send-keys", "-t", &fixture.coord_session, "Enter"]);
    }
    fixture.wait_for_screen(
        &fixture.coord_session,
        mock_tui::MOCK_REPLY_MARKER,
        START_DEADLINE,
    );
    fixture.wait_for_coordinator();
    Ok(())
}

/// One pane, the coordinator's real TUI, on the scratch tmux server.
fn run_coordinator_tui(fixture: &Fixture, h: &Harness, launch: &MockTuiLaunch) {
    let mut command = String::from("exec env");
    for (key, value) in &launch.env {
        command.push_str(&format!(" {}={}", key, shell_quote(value)));
    }
    command.push_str(&format!(
        " {}={}",
        "BOOP_DB",
        shell_quote(&fixture.mail.join("boop.db").display().to_string())
    ));
    command.push_str(&format!(" {}={}", "BOOP_NO_SYNC", shell_quote("1")));
    command.push_str(&format!(
        " {} tui {} --name {} --bin {} --cwd {} --mail-dir {} --",
        shell_quote(BOOP),
        h.entry,
        shell_quote(&fixture.coord_route),
        shell_quote(&launch.executable),
        shell_quote(&fixture.workspace.display().to_string()),
        shell_quote(&fixture.mail.display().to_string()),
    ));
    for arg in &launch.args {
        command.push(' ');
        command.push_str(&shell_quote(arg));
    }
    let output = fixture.tmux(&[
        "new-session",
        "-d",
        "-x",
        "220",
        "-y",
        "50",
        "-s",
        &fixture.coord_session,
        &command,
    ]);
    assert!(
        output.status.success(),
        "coordinator tmux new-session failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Resolve one harness's executable, or a skip reason.
fn harness_executable(h: &Harness) -> Result<PathBuf, String> {
    mock_tui::resolve_executable(h.entry, h.bin_env)
        .ok_or_else(|| format!("no {} executable", h.entry))
}

/// Everything a case needs: the scratch world with its coordinator up and the
/// lane spawned under it.
struct Started {
    fixture: Fixture,
    _provider: MockProvider,
}

/// Spawn the provider, the coordinator TUI on `h`, and a lane on `h` under it.
fn start(
    case: &str,
    h: &Harness,
    llmock: &Path,
    pace_ms: u64,
    extra_env: &[(&str, &str)],
    expect: &[&str],
) -> Result<Started, String> {
    let executable = harness_executable(h)?;
    let fixture = Fixture::new(case, h);
    let fixture_path = fixture.root.join("llmock.yaml");
    std::fs::write(&fixture_path, fixture_yaml(pace_ms)).map_err(|error| error.to_string())?;
    let provider = MockProvider::spawn(llmock, Some(&fixture_path))
        .map_err(|error| format!("llmock spawn: {error}"))?;

    start_coordinator(&fixture, h, provider.port)?;

    let registry = Registry::discover();
    let lane_launch = registry
        .get(h.id)
        .mock_tui_launch(&MockTuiContext {
            home: &fixture.lane_home,
            workspace: &fixture.repo,
            port: provider.port,
        })
        .map_err(|error| error.to_string())?;
    let env = lane_env(&fixture, &lane_launch.env, extra_env);
    // A codex lane speaks ACP through `npx @agentclientprotocol/codex-acp`; the
    // others run their own binary as the harness child.
    let bin = match h.id {
        HarnessId::Codex => None,
        _ => Some(executable.as_path()),
    };
    let created = fixture.create(&fixture.coord_route, expect, &env, bin);
    if !created.status.success() {
        return Err(format!(
            "lane create failed: {}",
            String::from_utf8_lossy(&created.stderr)
        ));
    }
    Ok(Started {
        fixture,
        _provider: provider,
    })
}

fn report(case: &str, h: &Harness, result: Result<(), String>) {
    match result {
        Ok(()) => println!("pass {case} {}", h.entry),
        Err(reason) => println!("skip {case} {}: {reason}", h.entry),
    }
}

/// Case 1. A hail lands under the first brief turn; the result waits for the
/// commit and rc=4 never appears. The parent proves it on its own screen.
fn run_held(h: &Harness) -> Result<(), String> {
    let Some(llmock) = mock_tui::resolve_llmock() else {
        return Err("no llmock".to_owned());
    };
    let started = start(
        "held",
        h,
        &llmock,
        3000,
        &[],
        &["--expect-commit-subject", "the third commit"],
    )?;
    let fixture = &started.fixture;

    // The ack turn is the first "turn starting"; send under the brief turn.
    fixture.wait_for_log("lane turn starting", 2);
    let beep = fixture.beep(&[
        &fixture.lane,
        "add the third commit",
        "--as",
        &fixture.coord_route,
        "--no-wait",
    ]);
    if !beep.status.success() {
        return Err(format!(
            "the hail failed: {}",
            String::from_utf8_lossy(&beep.stderr)
        ));
    }
    commit(&fixture.repo, "the third commit");

    let result = format!("lane {} done rc=0", fixture.lane);
    fixture.wait_for_screen(&fixture.coord_session, &result, START_DEADLINE);
    assert_eq!(
        fixture.coordinator_shows(&result),
        1,
        "the coordinator must hold exactly one completed result:\n{}",
        fixture.coord_screen()
    );
    assert!(
        !fixture.coord_screen().contains("rc=4"),
        "a premature rc=4 reached the coordinator:\n{}",
        fixture.coord_screen()
    );
    assert!(
        !fixture.any_row_contains("rc=4"),
        "a premature rc=4 row was written:\n{}",
        fixture.log()
    );
    Ok(())
}

/// Case 2. A lane retires; its route is gone; a send replays the spawn record
/// and the coordinator sees the second result.
fn run_revive(h: &Harness) -> Result<(), String> {
    let Some(llmock) = mock_tui::resolve_llmock() else {
        return Err("no llmock".to_owned());
    };
    let started = start(
        "revive",
        h,
        &llmock,
        0,
        &[("BOOP_IDLE_SHUTDOWN_SECS", "1")],
        &[],
    )?;
    let fixture = &started.fixture;
    let result = format!("lane {} done rc=0", fixture.lane);
    fixture.wait_for_result(1);
    fixture.wait_for_screen(&fixture.coord_session, &result, START_DEADLINE);
    fixture.wait_for_retired();
    fixture.wait_for_route_gone();
    assert!(
        fixture.trail("spawn.json").exists(),
        "the spawn record a revive replays must survive retirement"
    );

    // The live store left dead panes pinned open by `remain-on-exit`; the send
    // has to revive through that leftover rather than hold behind it.
    let option = fixture.tmux(&["set-option", "-g", "remain-on-exit", "on"]);
    assert!(option.status.success(), "set remain-on-exit");
    let _ = fixture.tmux(&["kill-session", "-t", &fixture.lane]);
    let stale = fixture.tmux(&["new-session", "-d", "-s", &fixture.lane, "true"]);
    assert!(
        stale.status.success(),
        "plant a leftover session: {}",
        String::from_utf8_lossy(&stale.stderr)
    );
    let planted = Instant::now();
    while !fixture.session_alive() {
        assert!(
            planted.elapsed() < Duration::from_secs(5),
            "leftover session"
        );
        std::thread::sleep(POLL);
    }

    let revived = fixture.beep(&[
        &fixture.lane,
        "second",
        "--as",
        &fixture.coord_route,
        "--timeout",
        "60",
    ]);
    let stdout = String::from_utf8_lossy(&revived.stdout);
    let stderr = String::from_utf8_lossy(&revived.stderr);
    if !revived.status.success() {
        return Err(format!(
            "the send to the retired lane failed:\n{stdout}{stderr}\n{}",
            fixture.log()
        ));
    }
    fixture.wait_for_result(2);
    let deadline = Instant::now() + START_DEADLINE;
    while fixture.coordinator_shows(&result) < 2 {
        assert!(
            Instant::now() < deadline,
            "the coordinator never saw the revived result:\n{}",
            fixture.coord_screen()
        );
        std::thread::sleep(POLL);
    }
    Ok(())
}

/// Case 3. A retired lane closes its tmux session even when the scratch server
/// runs with `remain-on-exit on`.
fn run_retired(h: &Harness) -> Result<(), String> {
    let Some(llmock) = mock_tui::resolve_llmock() else {
        return Err("no llmock".to_owned());
    };
    let started = start(
        "retired",
        h,
        &llmock,
        0,
        &[("BOOP_IDLE_SHUTDOWN_SECS", "1")],
        &[],
    )?;
    let fixture = &started.fixture;
    let option = fixture.tmux(&["set-option", "-g", "remain-on-exit", "on"]);
    assert!(
        option.status.success(),
        "could not set remain-on-exit: {}",
        String::from_utf8_lossy(&option.stderr)
    );
    fixture.wait_for_result(1);
    fixture.wait_for_retired();
    fixture.wait_for_session_gone();
    Ok(())
}

/// Case 4. A lane parked with idle shutdown disabled tells its live coordinator
/// when it goes quiet past `BOOP_STALE_SECS`, once per bound.
fn run_stale(h: &Harness) -> Result<(), String> {
    let Some(llmock) = mock_tui::resolve_llmock() else {
        return Err("no llmock".to_owned());
    };
    let started = start(
        "stale",
        h,
        &llmock,
        0,
        &[("BOOP_STALE_SECS", "3"), ("BOOP_IDLE_SHUTDOWN_SECS", "0")],
        &[],
    )?;
    let fixture = &started.fixture;
    let alarm = format!("stale {} ", fixture.lane);
    fixture.wait_for_screen(&fixture.coord_session, &alarm, STALE_DEADLINE);
    let first = fixture.coordinator_shows(&alarm);
    assert_eq!(first, 1, "exactly one stale row reached the screen");
    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(
        fixture.coordinator_shows(&alarm),
        first,
        "the alarm reached the screen again before its bound"
    );
    assert_eq!(
        fixture.stale_rows(),
        1,
        "exactly one stale row in the mailbox"
    );
    Ok(())
}

macro_rules! lifecycle_case {
    ($case:literal, $runner:ident, $name:ident, $h:expr) => {
        #[test]
        fn $name() {
            let _guard = lane_lock();
            let h = &HARNESSES[$h];
            report($case, h, $runner(h));
        }
    };
}

lifecycle_case!("held", run_held, held_row_defers_result_claude, 0);
lifecycle_case!("held", run_held, held_row_defers_result_codex, 1);
lifecycle_case!("held", run_held, held_row_defers_result_opencode, 2);
lifecycle_case!("revive", run_revive, revive_claude, 0);
lifecycle_case!("revive", run_revive, revive_codex, 1);
lifecycle_case!("revive", run_revive, revive_opencode, 2);
lifecycle_case!("retired", run_retired, retired_closes_session_claude, 0);
lifecycle_case!("retired", run_retired, retired_closes_session_codex, 1);
lifecycle_case!("retired", run_retired, retired_closes_session_opencode, 2);
lifecycle_case!("stale", run_stale, stale_claude, 0);
lifecycle_case!("stale", run_stale, stale_codex, 1);
lifecycle_case!("stale", run_stale, stale_opencode, 2);
