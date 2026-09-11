//! Lane lifecycle end to end: the real boop binary spawns real lanes whose
//! opencode harness runs against a loopback llmock provider. The tmux server is
//! a scratch one, deliberately running with `remain-on-exit on` so a paneless
//! supervisor cannot quietly pass a session-exit assertion.
//!
//! Three defects, three tests:
//! 1. a held inbound row defers the lane's result;
//! 2. a send to a retired lane revives it on its pinned conversation;
//! 3. a retired lane closes its tmux session.
//!
//! Skips when llmock or opencode is absent, like `commit_push_e2e.rs`:
//!   cargo install --git https://github.com/larsakerlund/llmock.git \
//!     --tag v0.1.2 --locked llmock
//! Executable override: OPENCODE_BIN, LLMOCK_BIN.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use boop::harness::mock_tui::{self, MockProvider, MockTuiContext, MockTuiLaunch};
use boop::harness::{shell_quote, HarnessId};
use boop::Registry;
use boop_store::testing::BoopCommandExt;

const BOOP: &str = env!("CARGO_BIN_EXE_boop");

/// The lane tests each spawn a real harness and a tmux server; run them one at
/// a time so a loaded machine does not starve a turn past its deadline.
static LANE_LOCK: Mutex<()> = Mutex::new(());

fn lane_lock() -> std::sync::MutexGuard<'static, ()> {
    LANE_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

const POLL: Duration = Duration::from_millis(100);
const START_DEADLINE: Duration = Duration::from_secs(60);

/// A scratch world: repo, mailbox, lane home, scratch tmux server.
struct Fixture {
    root: PathBuf,
    repo: PathBuf,
    brief: PathBuf,
    mail: PathBuf,
    home: PathBuf,
    bin: PathBuf,
    socket: String,
    lane: String,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let root =
            std::env::temp_dir().join(format!("boop-lifecycle-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("repo");
        let mail = root.join("mail");
        let home = root.join("home");
        let bin = root.join("bin");
        for dir in [&repo, &mail, &home, &bin] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::create_dir_all(root.join("config/boop")).unwrap();
        std::fs::write(root.join("config/boop/config.json"), "{}").unwrap();
        std::fs::write(
            mail.join("registry.json"),
            serde_json::json!({ "obs": { "kind": "coordinator" } }).to_string(),
        )
        .unwrap();
        let brief = root.join("brief.md");
        std::fs::write(&brief, "finish the brief and report\n").unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["config", "user.email", "boop@example.invalid"]);
        git(&repo, &["config", "user.name", "Boop Lifecycle"]);
        std::fs::write(repo.join("seed.txt"), "seed\n").unwrap();
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-qm", "seed"]);
        std::os::unix::fs::symlink(BOOP, bin.join("boop")).unwrap();
        Fixture {
            root,
            repo,
            brief,
            mail,
            home,
            bin,
            socket: format!("boop-lifecycle-{}-{name}", std::process::id()),
            lane: format!("feature-lifecycle-{name}"),
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

    /// One `lane create`, pointed at the mock provider's env and executable.
    fn create(
        &self,
        parent: &str,
        extra: &[&str],
        env: &[(String, String)],
        executable: &Path,
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
            .args(["--harness", "opencode", "--model", "llmock/mock-model"])
            .arg("--parent")
            .arg(parent)
            .arg("--tmux")
            .arg(&self.lane)
            .arg("--socket")
            .arg(&self.socket)
            .arg("--mail-dir")
            .arg(&self.mail)
            .arg("--bin")
            .arg(executable)
            .arg("--no-start");
        for (key, value) in env {
            command.arg("--env").arg(format!("{key}={value}"));
        }
        command.args(extra);
        command.output().expect("run boop lane create")
    }

    /// Start a real codex coordinator TUI in a pane on the scratch server, so a
    /// lane's door row has a parent harness to land on.
    fn run_coordinator_tui(
        &self,
        route: &str,
        session: &str,
        launch: &MockTuiLaunch,
        workspace: &Path,
    ) {
        let mut command = String::from("exec env");
        for (key, value) in &launch.env {
            command.push_str(&format!(" {}={}", key, shell_quote(value)));
        }
        command.push_str(&format!(
            " {}={}",
            "BOOP_DB",
            shell_quote(&self.mail.join("boop.db").display().to_string())
        ));
        command.push_str(&format!(" {}={}", "BOOP_NO_SYNC", shell_quote("1")));
        command.push_str(&format!(
            " {} tui codex --name {} --bin {} --cwd {} --mail-dir {} --",
            shell_quote(BOOP),
            shell_quote(route),
            shell_quote(&launch.executable),
            shell_quote(&workspace.display().to_string()),
            shell_quote(&self.mail.display().to_string()),
        ));
        for arg in &launch.args {
            command.push(' ');
            command.push_str(&shell_quote(arg));
        }
        let output = self.tmux(&[
            "new-session",
            "-d",
            "-x",
            "200",
            "-y",
            "50",
            "-s",
            session,
            &command,
        ]);
        assert!(
            output.status.success(),
            "coordinator tmux new-session failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn screen(&self, session: &str) -> String {
        let output = self.tmux(&["capture-pane", "-p", "-t", session, "-S", "-400"]);
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn wait_for_screen(&self, session: &str, wanted: &str) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let text = self.screen(session);
            if text.contains(wanted) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "coordinator never showed {wanted:?}\n{text}"
            );
            std::thread::sleep(POLL);
        }
    }

    /// Wait for a coordinator route to bind a session or app-server socket.
    fn wait_for_coordinator(&self, route: &str) {
        let deadline = Instant::now() + START_DEADLINE;
        loop {
            let routes = boop_store::testing::routes_json(&self.mail.join("boop.db"));
            if let Some(entry) = routes.get(route) {
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
                "coordinator route {route} never bound a session"
            );
            std::thread::sleep(POLL);
        }
    }

    /// Alarm rows this lane wrote.
    fn stale_rows(&self) -> usize {
        self.rows()
            .into_iter()
            .filter(|row| row.get("kind").and_then(|v| v.as_str()) == Some("stale"))
            .filter(|row| row.get("from").and_then(|v| v.as_str()) == Some(self.lane.as_str()))
            .count()
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
}

impl Drop for Fixture {
    fn drop(&mut self) {
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

/// The provider fixture. The readiness probe answers `boop`; every other turn
/// replies with text long enough that the supervisor treats the brief turn as
/// real work rather than an empty re-feed. `pace_ms` delays the stream so a turn
/// stays open long enough for the test to slip a hail in under it.
fn fixture_yaml(pace_ms: u64) -> String {
    format!(
        r#"rules:
  - match:
      user_contains: "Respond exactly with: boop"
    respond:
      content: "boop"
      stream:
        ttft_ms: {pace_ms}
        inter_token_ms: 0
  - match: {{}}
    respond:
      content: "the brief turn finished with a reply long enough to count as real work"
      stream:
        ttft_ms: {pace_ms}
        inter_token_ms: 0
"#
    )
}

/// The lane's scratch env: the mock recipe's own env, plus the test's `boop` on
/// PATH and any caller extras.
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

/// Spawn the llmock provider and the opencode mock recipe for one fixture.
fn provider_and_launch(
    fixture: &Fixture,
    llmock: &Path,
    pace_ms: u64,
) -> Option<(MockProvider, PathBuf, Vec<(String, String)>)> {
    let registry = Registry::discover();
    let executable = mock_tui::resolve_executable("opencode", "OPENCODE_BIN")?;
    let fixture_path = fixture.root.join("llmock.yaml");
    std::fs::write(&fixture_path, fixture_yaml(pace_ms)).unwrap();
    let provider = MockProvider::spawn(llmock, Some(&fixture_path)).ok()?;
    let launch = registry
        .get(HarnessId::Opencode)
        .mock_tui_launch(&MockTuiContext {
            home: &fixture.home,
            workspace: &fixture.repo,
            port: provider.port,
        })
        .ok()?;
    Some((provider, executable, launch.env))
}

/// RECEIPT. A hail held at the first turn boundary defers the lane's result:
/// the supervisor feeds the row, the test's commit lands under that deferred
/// turn, and the only result is rc=0. Sabotage: writing the result at the
/// first turn end leaves an rc=4 row and no second turn.
#[test]
fn a_held_row_defers_the_lane_result() {
    let _lane = lane_lock();
    let Some(llmock) = mock_tui::resolve_llmock() else {
        eprintln!("skip: no llmock");
        return;
    };
    if mock_tui::resolve_executable("opencode", "OPENCODE_BIN").is_none() {
        eprintln!("skip: no opencode");
        return;
    }
    let fixture = Fixture::new("held");
    let Some((_provider, executable, launch_env)) = provider_and_launch(&fixture, &llmock, 4000)
    else {
        eprintln!("skip: no opencode mock recipe");
        return;
    };
    let env = lane_env(&fixture, &launch_env, &[]);
    let created = fixture.create(
        "obs",
        &["--expect-commit-subject", "the third commit"],
        &env,
        &executable,
    );
    assert!(
        created.status.success(),
        "lane create failed: {}",
        String::from_utf8_lossy(&created.stderr)
    );

    // The ack turn is the first "turn starting"; wait for the brief turn so the
    // hail lands under a running turn and is held, not folded into the brief.
    fixture.wait_for_log("lane turn starting", 2);
    let beep = fixture.beep(&[
        &fixture.lane,
        "add the third commit",
        "--as",
        "obs",
        "--no-wait",
    ]);
    assert!(
        beep.status.success(),
        "the hail failed: {}",
        String::from_utf8_lossy(&beep.stderr)
    );

    // The brief turn ends next; a supervisor that holds the row defers its
    // result and opens a third turn for it. Only then does the test commit, so
    // an early rc=4 would already have been written and read below.
    fixture.wait_for_log("lane turn starting", 3);
    assert_eq!(
        fixture.result_bodies().len(),
        0,
        "a result was written before the held turn ran:\n{}",
        fixture.log()
    );
    commit(&fixture.repo, "the third commit");
    fixture.wait_for_result(1);

    let bodies = fixture.result_bodies();
    assert_eq!(
        bodies.len(),
        1,
        "the lane must write exactly one result row: {bodies:?}"
    );
    assert!(
        bodies[0].contains("rc=0"),
        "the lone result must be the completed lane: {bodies:?}"
    );
    assert!(
        !fixture.any_row_contains("rc=4"),
        "a premature rc=4 row was written:\n{}",
        fixture.log()
    );
}

/// RECEIPT. A lane retires; its route is gone; a send replays the spawn record,
/// re-registers the route, resumes the conversation and writes a fresh result.
/// Sabotage: leaving the route unwritten holds the body instead of reviving.
#[test]
fn a_send_to_a_retired_lane_revives_it() {
    let _lane = lane_lock();
    let Some(llmock) = mock_tui::resolve_llmock() else {
        eprintln!("skip: no llmock");
        return;
    };
    if mock_tui::resolve_executable("opencode", "OPENCODE_BIN").is_none() {
        eprintln!("skip: no opencode");
        return;
    }
    let fixture = Fixture::new("revive");
    let Some((_provider, executable, launch_env)) = provider_and_launch(&fixture, &llmock, 0)
    else {
        eprintln!("skip: no opencode mock recipe");
        return;
    };
    let env = lane_env(&fixture, &launch_env, &[("BOOP_IDLE_SHUTDOWN_SECS", "1")]);
    let created = fixture.create("obs", &[], &env, &executable);
    assert!(
        created.status.success(),
        "lane create failed: {}",
        String::from_utf8_lossy(&created.stderr)
    );
    fixture.wait_for_result(1);
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
    // Clear whatever the run left; the point is a dead pane pinned open, not
    // which run left it.
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

    let revived = fixture.beep(&[&fixture.lane, "second", "--as", "obs", "--timeout", "60"]);
    let stdout = String::from_utf8_lossy(&revived.stdout);
    let stderr = String::from_utf8_lossy(&revived.stderr);
    assert!(
        revived.status.success(),
        "the send to the retired lane failed:\n{stdout}{stderr}\n{}",
        fixture.log()
    );
    assert!(
        stdout.contains(&format!("revive {}", fixture.lane)),
        "no revive line:\n{stdout}"
    );
    assert!(
        stdout.contains(&format!("revived {}", fixture.lane)),
        "no revived line:\n{stdout}"
    );
    fixture.wait_for_result(2);
    let bodies = fixture.result_bodies();
    assert!(
        bodies.last().is_some_and(|body| body.contains("rc=0")),
        "the revived lane must write a fresh rc=0 result: {bodies:?}"
    );
}

/// RECEIPT. A retired lane closes its tmux session even when the scratch server
/// runs with `remain-on-exit on`, which would otherwise pin the dead pane.
/// Sabotage: relying on the pane command's exit leaves the session alive.
#[test]
fn a_retired_lane_closes_its_tmux_session() {
    let _lane = lane_lock();
    let Some(llmock) = mock_tui::resolve_llmock() else {
        eprintln!("skip: no llmock");
        return;
    };
    if mock_tui::resolve_executable("opencode", "OPENCODE_BIN").is_none() {
        eprintln!("skip: no opencode");
        return;
    }
    let fixture = Fixture::new("pane");
    let Some((_provider, executable, launch_env)) = provider_and_launch(&fixture, &llmock, 0)
    else {
        eprintln!("skip: no opencode mock recipe");
        return;
    };
    let env = lane_env(&fixture, &launch_env, &[("BOOP_IDLE_SHUTDOWN_SECS", "1")]);
    let created = fixture.create("obs", &[], &env, &executable);
    assert!(
        created.status.success(),
        "lane create failed: {}",
        String::from_utf8_lossy(&created.stderr)
    );
    // The condition the live store ran under: a server that keeps dead panes.
    let option = fixture.tmux(&["set-option", "-g", "remain-on-exit", "on"]);
    assert!(
        option.status.success(),
        "could not set remain-on-exit: {}",
        String::from_utf8_lossy(&option.stderr)
    );
    fixture.wait_for_result(1);
    fixture.wait_for_retired();
    fixture.wait_for_session_gone();
}

/// RECEIPT. A lane parked with idle shutdown disabled tells its parent when it
/// has been quiet past `BOOP_STALE_SECS`. The alarm takes the coordinator's
/// door, not a mailbox-only progress rung, and repeats no faster than the
/// bound. Sabotage: classifying the row as a progress row leaves the
/// coordinator pane silent.
#[test]
fn a_stale_lane_tells_its_parent() {
    let _lane = lane_lock();
    let Some(llmock) = mock_tui::resolve_llmock() else {
        eprintln!("skip: no llmock");
        return;
    };
    if mock_tui::resolve_executable("opencode", "OPENCODE_BIN").is_none() {
        eprintln!("skip: no opencode");
        return;
    }
    if mock_tui::resolve_executable("codex", "CODEX_BIN").is_none() {
        eprintln!("skip: no codex coordinator");
        return;
    }
    let fixture = Fixture::new("stale");
    let fixture_path = fixture.root.join("llmock.yaml");
    std::fs::write(&fixture_path, fixture_yaml(0)).unwrap();
    let provider = MockProvider::spawn(&llmock, Some(&fixture_path)).expect("spawn llmock");
    let registry = Registry::discover();

    // The parent is a real codex coordinator TUI, so the door the alarm must
    // take is a real harness door.
    let coord_route = "coord-stale";
    let coord_session = format!("{coord_route}-{}", std::process::id());
    let coord_home = fixture.root.join("coord-home");
    let workspace = fixture.root.join("workspace");
    std::fs::create_dir_all(&coord_home).unwrap();
    std::fs::create_dir_all(&workspace).unwrap();
    let codex = registry
        .get(HarnessId::Codex)
        .mock_tui_launch(&MockTuiContext {
            home: &coord_home,
            workspace: &workspace,
            port: provider.port,
        })
        .expect("codex mock recipe");
    fixture.run_coordinator_tui(coord_route, &coord_session, &codex, &workspace);
    fixture.wait_for_coordinator(coord_route);

    // The lane never retires: `BOOP_IDLE_SHUTDOWN_SECS=0` plus a short stale
    // bound is exactly the live-store trap.
    let opencode = mock_tui::resolve_executable("opencode", "OPENCODE_BIN").unwrap();
    let lane_launch = registry
        .get(HarnessId::Opencode)
        .mock_tui_launch(&MockTuiContext {
            home: &fixture.home,
            workspace: &fixture.repo,
            port: provider.port,
        })
        .expect("opencode mock recipe");
    let env = lane_env(
        &fixture,
        &lane_launch.env,
        &[("BOOP_STALE_SECS", "3"), ("BOOP_IDLE_SHUTDOWN_SECS", "0")],
    );
    let created = fixture.create(coord_route, &[], &env, &opencode);
    assert!(
        created.status.success(),
        "lane create failed: {}",
        String::from_utf8_lossy(&created.stderr)
    );

    // Within 10s the coordinator screen shows the alarm, and it took the door.
    fixture.wait_for_screen(&coord_session, &format!("stale {}", fixture.lane));
    let first = fixture.stale_rows();
    assert_eq!(first, 1, "the lane wrote exactly one alarm row");
    // No second alarm inside the next 2s: the bound is the repeat floor.
    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(
        fixture.stale_rows(),
        first,
        "the alarm repeated before its bound"
    );
}
