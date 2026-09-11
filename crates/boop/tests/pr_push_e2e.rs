//! PR push end to end: the real boop binary, a real lane supervisor running a
//! real harness, and the mock-TUI provider recipe. The only stubs are llmock
//! for the model and a `gh` script on PATH for GitHub.
//!
//! The lane's claude runs `gh pr create` because llmock answers the brief turn
//! with a Bash tool call; claude then writes a `pr-link` record into its own
//! transcript. That gives both producers: the supervisor reads `gh pr view`,
//! and transcript ingest reads the pr-link.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use boop::bus;
use boop::harness::mock_tui;
use boop_store::testing::BoopCommandExt;

const BOOP: &str = env!("CARGO_BIN_EXE_boop");
const PR_URL: &str = "https://github.com/acme/widget/pull/7";

/// A real claude lane is heavyweight; run the lane tests one at a time so a
/// loaded machine does not starve a turn past its deadline.
static LANE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn lane_lock() -> std::sync::MutexGuard<'static, ()> {
    LANE_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

/// What the gh stub does for `pr view`.
#[derive(Clone, Copy)]
enum GhView {
    Answers,
    Fails,
    Hangs,
}

struct Fixture {
    root: PathBuf,
    repo: PathBuf,
    brief: PathBuf,
    mail: PathBuf,
    bin: PathBuf,
    home: PathBuf,
    socket: String,
    tmux: String,
    lane: String,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        Self::with_gh(name, GhView::Answers)
    }

    fn with_gh(name: &str, view: GhView) -> Fixture {
        let root = std::env::temp_dir().join(format!("boop-prpush-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("repo");
        let mail = root.join("mail");
        let bin = root.join("bin");
        let home = root.join("home");
        for dir in [&repo, &mail, &bin, &home] {
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
        std::fs::write(&brief, "finish and report\n").unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["config", "user.email", "boop@example.invalid"]);
        git(&repo, &["config", "user.name", "Boop Test"]);
        std::fs::write(repo.join("seed.txt"), "seed\n").unwrap();
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-qm", "seed"]);
        std::os::unix::fs::symlink(BOOP, bin.join("boop")).unwrap();
        make_executable(&bin.join("gh"), &gh_script(view));
        Fixture {
            root,
            repo,
            brief,
            mail,
            bin,
            home,
            socket: format!("boop-prpush-{}-{name}", std::process::id()),
            tmux: format!("feature-prpush-{name}"),
            lane: format!("feature-prpush-{name}"),
        }
    }

    fn write_config(&self, text: &str) {
        std::fs::write(self.root.join("config/boop/config.json"), text).unwrap();
    }

    fn create(&self, extra: &[&str]) -> std::process::Output {
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
            .args(["--harness", "claude", "--model", "sonnet"])
            .arg("--parent")
            .arg("obs")
            .arg("--tmux")
            .arg(&self.tmux)
            .arg("--socket")
            .arg(&self.socket)
            .arg("--mail-dir")
            .arg(&self.mail)
            .arg("--no-start");
        command.args(extra);
        command.output().expect("run boop lane create")
    }

    fn db(&self, sql: &str) -> String {
        let output = Command::new(BOOP)
            .boop_test_root(&self.root)
            .env("BOOP_DB", self.mail.join("boop.db"))
            .env("BOOP_NO_SYNC", "1")
            .args(["db", sql])
            .output()
            .expect("run boop db");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// One sync pass with the scratch home as the reader root, so it finds the
    /// lane's claude transcript.
    fn sync(&self) -> String {
        let output = Command::new(BOOP)
            .boop_test_root(&self.root)
            .env("BOOP_DB", self.mail.join("boop.db"))
            .env("BOOP_READER_HOME", &self.home)
            .args(["db", "sync", "create"])
            .output()
            .expect("run boop db sync create");
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }

    fn tmux(&self, args: &[&str]) -> std::process::Output {
        Command::new("tmux")
            .arg("-L")
            .arg(&self.socket)
            .args(args)
            .output()
            .expect("run tmux")
    }

    fn llmock_fixture(&self) -> PathBuf {
        let path = self.root.join("llmock.yaml");
        std::fs::write(&path, llmock_fixture()).unwrap();
        path
    }

    /// The scratch stubs and provider env a claude lane inherits.
    fn lane_env(&self) -> Vec<String> {
        let path = std::env::var("PATH").unwrap_or_default();
        vec![
            format!("PATH={}:{path}", self.bin.display()),
            format!("HOME={}", self.home.display()),
            format!("CLAUDE_CONFIG_DIR={}", self.home.join(".claude").display()),
            "DISABLE_AUTOUPDATER=1".to_owned(),
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1".to_owned(),
        ]
    }

    fn spawn_env(&self, provider_port: u16) -> Vec<String> {
        let mut env = self.lane_env();
        env.push(format!(
            "ANTHROPIC_BASE_URL=http://127.0.0.1:{provider_port}/anthropic"
        ));
        env.push("ANTHROPIC_AUTH_TOKEN=test".to_owned());
        env
    }

    /// The claude session id the lane's transcript carries, from the pinned
    /// conversation the supervisor wrote.
    fn lane_session(&self) -> Option<String> {
        let text = std::fs::read_to_string(
            self.mail
                .join("lanes")
                .join(&self.lane)
                .join("conversation"),
        )
        .ok()?;
        serde_json::from_str::<serde_json::Value>(&text)
            .ok()?
            .get("conversation")?
            .as_str()
            .map(str::to_owned)
    }

    /// Register a route naming the lane's session, as a live coordinator or a
    /// native wrapper would. The lane's own route is dropped when it exits.
    fn seed_session_route(&self, session: &str) {
        let route = bus::Route {
            kind: "lane".into(),
            harness: None,
            tmux: None,
            cwd: None,
            model: None,
            mode: None,
            session_id: Some(session.to_owned()),
            source_path: None,
            parent: Some("obs".into()),
            goal: None,
            registered_at: None,
            base_sha: None,
            worktree_dir: None,
            app_server_socket: None,
        };
        bus::write_route(&self.mail, "lane-session", &route).unwrap();
    }

    fn supervise_log(&self) -> String {
        std::fs::read_to_string(
            self.mail
                .join("lanes")
                .join(&self.lane)
                .join("supervise.log"),
        )
        .unwrap_or_default()
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

fn make_executable(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// The gh stub: `pr create` prints a URL; `pr view` answers, fails, or hangs.
fn gh_script(view: GhView) -> String {
    let view = match view {
        GhView::Answers => {
            format!("  \"pr view\") echo '{{\"url\":\"{PR_URL}\",\"title\":\"t\"}}'; exit 0;;")
        }
        GhView::Fails => "  \"pr view\") exit 1;;".to_owned(),
        GhView::Hangs => "  \"pr view\") sleep 30; exit 0;;".to_owned(),
    };
    format!(
        "#!/bin/sh\ncase \"$1 $2\" in\n  \"pr create\") echo '{PR_URL}'; exit 0;;\n{view}\nesac\nexit 1\n"
    )
}

/// The provider fixture: the readiness probe answers `boop`; the brief turn
/// answers a Bash `gh pr create` tool call; anything else is a plain reply.
fn llmock_fixture() -> &'static str {
    r#"rules:
  - match:
      user_contains: "transport readiness"
    respond:
      content: "boop"
  - match:
      user_contains: "gh pr create --fill"
    respond:
      tool_calls:
        - name: Bash
          arguments:
            command: "gh pr create --fill --base main"
  - match: {}
    respond:
      content: "done"
"#
}

/// Write the claude onboarding config a stream-json claude child needs.
fn write_claude_home(home: &Path, workspace: &Path) {
    let claude = home.join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    let projects = serde_json::json!({
        workspace.display().to_string(): { "hasTrustDialogAccepted": true },
    });
    std::fs::write(
        claude.join(".claude.json"),
        serde_json::json!({
            "firstStartTime": "2026-01-01T00:00:00.000Z",
            "firstStartVersion": "2",
            "hasCompletedOnboarding": true,
            "projects": projects,
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(claude.join("settings.json"), "{}").unwrap();
}

/// The count from one `SELECT COUNT(*) AS n ...` query, read as `boop db`'s
/// compact JSON rows.
fn row_count(fixture: &Fixture, kind: &str) -> i64 {
    fixture
        .db(&format!(
            "SELECT COUNT(*) AS n FROM agent_mail WHERE kind = '{kind}'"
        ))
        .lines()
        .find_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .and_then(|value| value.get("n").and_then(serde_json::Value::as_i64))
        .unwrap_or(0)
}

fn wait_for_row(fixture: &Fixture, kind: &str, want: i64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        if row_count(fixture, kind) >= want {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// Start a real claude lane against the mock provider and wait for it to end.
fn run_lane(fixture: &Fixture, llmock: &Path, extra_env: &[String]) {
    let Some(claude) = mock_tui::resolve_executable("claude", "CLAUDE_BIN") else {
        panic!("claude must be on PATH for this test");
    };
    write_claude_home(&fixture.home, &fixture.repo);
    let provider = mock_tui::MockProvider::spawn(llmock, Some(&fixture.llmock_fixture()))
        .expect("spawn llmock");
    let mut args: Vec<String> = fixture
        .spawn_env(provider.port)
        .into_iter()
        .chain(extra_env.iter().cloned())
        .map(|pair| format!("--env={pair}"))
        .collect();
    args.push(format!("--bin={}", claude.display()));
    args.push("--post-pr".to_owned());
    args.push("--pr-base".to_owned());
    args.push("main".to_owned());
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let created = fixture.create(&refs);
    assert!(
        created.status.success(),
        "lane create failed: {}",
        String::from_utf8_lossy(&created.stderr)
    );
    assert!(
        wait_for_row(fixture, "result", 1),
        "the lane never wrote a result row"
    );
}

/// RECEIPT. `lane create --post-pr --pr-base` prints the toggle on the dry-run
/// line; absent the flag it prints `off`, and `--no-post-pr` overrides config.
#[test]
fn lane_create_dry_run_prints_the_post_pr_toggle() {
    let fixture = Fixture::new("toggle");
    let on = fixture.create(&["--post-pr", "--pr-base", "dev", "--dry-run"]);
    let stdout = String::from_utf8_lossy(&on.stdout);
    assert!(on.status.success(), "{stdout}");
    assert!(stdout.contains("post-pr: dev"), "{stdout}");

    let off = fixture.create(&["--dry-run"]);
    assert!(
        String::from_utf8_lossy(&off.stdout).contains("post-pr: off"),
        "{}",
        String::from_utf8_lossy(&off.stdout)
    );

    fixture.write_config(r#"{ "post-pr": true, "pr-base": "release" }"#);
    let configured = fixture.create(&["--dry-run"]);
    assert!(
        String::from_utf8_lossy(&configured.stdout).contains("post-pr: release"),
        "{}",
        String::from_utf8_lossy(&configured.stdout)
    );
    let overridden = fixture.create(&["--no-post-pr", "--dry-run"]);
    assert!(
        String::from_utf8_lossy(&overridden.stdout).contains("post-pr: off"),
        "{}",
        String::from_utf8_lossy(&overridden.stdout)
    );
}

/// RECEIPT (supervisor producer). A real lane runs a real claude against
/// llmock; the model calls `gh pr create`, the supervisor reads `gh pr view`
/// and appends one kind=pr row to the lane's parent.
#[test]
fn lane_supervisor_pushes_a_new_pr() {
    let Some(llmock) = mock_tui::resolve_llmock() else {
        eprintln!("skipping: no llmock");
        return;
    };
    if mock_tui::resolve_executable("claude", "CLAUDE_BIN").is_none() {
        eprintln!("skipping: no claude");
        return;
    }
    let _lane = lane_lock();
    let fixture = Fixture::new("lane");
    run_lane(&fixture, &llmock, &[]);
    assert!(
        wait_for_row(&fixture, "pr", 1),
        "the supervisor never pushed a kind=pr row\n{}",
        fixture.db("SELECT kind, to_route, body FROM agent_mail")
    );
    let rows = fixture.db("SELECT to_route, body FROM agent_mail WHERE kind = 'pr'");
    assert!(
        rows.contains("obs"),
        "the PR row did not address obs:\n{rows}"
    );
    assert!(
        rows.contains(PR_URL),
        "the PR row body lacks the url:\n{rows}"
    );
    // A pr row is typed mail, not a supervisor progress row: the ladder must
    // not have stopped it at the progress-row rung.
    let transitions = fixture.db("SELECT detail FROM agent_delivery_transition");
    assert!(
        !transitions.contains("pr row; no door"),
        "the pr row stopped like a progress row:\n{transitions}"
    );
}

/// RECEIPT (ingest producer). The supervisor's `gh pr view` fails, so only
/// transcript ingest sees the PR: syncing the lane's own pr-link appends the
/// held kind=pr row for the session route.
#[test]
fn ingest_producer_pushes_a_new_pr() {
    let Some(llmock) = mock_tui::resolve_llmock() else {
        eprintln!("skipping: no llmock");
        return;
    };
    if mock_tui::resolve_executable("claude", "CLAUDE_BIN").is_none() {
        eprintln!("skipping: no claude");
        return;
    }
    let _lane = lane_lock();
    let fixture = Fixture::with_gh("ingest", GhView::Fails);
    run_lane(&fixture, &llmock, &[]);
    assert_eq!(
        row_count(&fixture, "pr"),
        0,
        "the supervisor producer must not have pushed"
    );
    let session = fixture.lane_session().expect("the lane pinned its session");
    fixture.seed_session_route(&session);
    fixture.sync();
    assert!(
        wait_for_row(&fixture, "pr", 1),
        "ingest never pushed the pr row\n{}",
        fixture.db("SELECT kind, to_route, body FROM agent_mail")
    );
    let rows = fixture.db("SELECT to_route, body FROM agent_mail WHERE kind = 'pr'");
    assert!(rows.contains("obs"), "{rows}");
    assert!(rows.contains(PR_URL), "{rows}");
}

/// RECEIPT (both producers). The supervisor pushes first; a later sync of the
/// same pr-link claims nothing new, so one notice reaches the subscriber.
#[test]
fn both_producers_keep_one_notice() {
    let Some(llmock) = mock_tui::resolve_llmock() else {
        eprintln!("skipping: no llmock");
        return;
    };
    if mock_tui::resolve_executable("claude", "CLAUDE_BIN").is_none() {
        eprintln!("skipping: no claude");
        return;
    }
    let _lane = lane_lock();
    let fixture = Fixture::new("both");
    run_lane(&fixture, &llmock, &[]);
    assert!(
        wait_for_row(&fixture, "pr", 1),
        "the supervisor pushed nothing"
    );
    let session = fixture.lane_session().expect("the lane pinned its session");
    fixture.seed_session_route(&session);
    fixture.sync();
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(
        row_count(&fixture, "pr"),
        1,
        "the second producer appended a duplicate notice\n{}",
        fixture.db("SELECT kind, to_route, body FROM agent_mail")
    );
}

/// RECEIPT (gh hangs). A `gh pr view` that never answers is killed at the
/// deadline: the lane still ends, no PR row is pushed, and the timeout warns.
#[test]
fn a_hung_gh_does_not_stall_the_lane() {
    let Some(llmock) = mock_tui::resolve_llmock() else {
        eprintln!("skipping: no llmock");
        return;
    };
    if mock_tui::resolve_executable("claude", "CLAUDE_BIN").is_none() {
        eprintln!("skipping: no claude");
        return;
    }
    let _lane = lane_lock();
    let fixture = Fixture::with_gh("hang", GhView::Hangs);
    let started = Instant::now();
    run_lane(
        &fixture,
        &llmock,
        &["BOOP_PR_VIEW_TIMEOUT_SECS=1".to_owned()],
    );
    assert!(
        started.elapsed() < Duration::from_secs(60),
        "the hung gh stalled the lane: {:?}",
        started.elapsed()
    );
    assert_eq!(
        row_count(&fixture, "pr"),
        0,
        "a hung gh must not push a PR row"
    );
    assert!(
        fixture.supervise_log().contains("gh pr view timed out"),
        "the timeout was not warned:\n{}",
        fixture.supervise_log()
    );
}
