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

use boop::harness::mock_tui::{self, MockProvider, MockTuiContext};
use boop::harness::HarnessId;
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
            .arg("obs")
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
