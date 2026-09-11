//! Commit-push end to end: a real lane supervisor watches a commit in its
//! worktree and the row reaches a real coordinator TUI through that harness's
//! own door, once. One pass per coordinator harness (claude, codex, opencode,
//! kimi) against a loopback llmock provider; the model provider is the only
//! seam, every other part is the real binary and the real TUI.
//!
//! Skips a harness when its executable, or `llmock`, is absent, exactly like
//! `shout_interrupt.rs`:
//!   cargo install --git https://github.com/larsakerlund/llmock.git \
//!     --tag v0.1.2 --locked llmock
//! Executable overrides: CODEX_BIN, CLAUDE_BIN (ccz rides this),
//! OPENCODE_BIN, KIMI_BIN, LLMOCK_BIN.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use boop::harness::mock_tui::{self, MockTuiLaunch, MockTuiReplay};
use boop::harness::{shell_quote, HarnessId};
use boop::Registry;

const BOOP: &str = env!("CARGO_BIN_EXE_boop");

/// The commit-push deadline for one step.
const STEP_DEADLINE: Duration = Duration::from_secs(30);
/// The poll interval under every deadline loop.
const POLL: Duration = Duration::from_millis(250);

/// llmock answers the supervisor's readiness probe with `boop` and every other
/// turn with the canned terminal reply the coordinator driver waits on.
const FIXTURE_YAML: &str = r#"rules:
  - match:
      user_contains: "Respond exactly with: boop"
    respond:
      content: "boop"
  - match: {}
    respond:
      content: |-
        FIXED_TERMINAL_REPLY
"#;

/// One harness's place in the matrix.
struct Case {
    entry: &'static str,
    id: HarnessId,
    executable_override: &'static str,
}

const CASES: &[Case] = &[
    Case {
        entry: "claude",
        id: HarnessId::Claude,
        executable_override: "CLAUDE_BIN",
    },
    Case {
        entry: "codex",
        id: HarnessId::Codex,
        executable_override: "CODEX_BIN",
    },
    Case {
        entry: "opencode",
        id: HarnessId::Opencode,
        executable_override: "OPENCODE_BIN",
    },
    Case {
        entry: "kimi",
        id: HarnessId::Kimi,
        executable_override: "KIMI_BIN",
    },
];

/// One harness's scratch world: home, repo, mailbox, coordinator session, lane.
struct Scratch {
    root: PathBuf,
    mail: PathBuf,
    repo: PathBuf,
    coordinator_session: String,
    coordinator_route: &'static str,
    lane: String,
}

impl Scratch {
    fn boop(&self, args: &[&str]) -> std::process::Output {
        use boop_store::testing::BoopCommandExt;
        Command::new(BOOP)
            .args(args)
            .boop_test_root(&self.root)
            .env("BOOP_DB", self.root.join("mail").join("boop.db"))
            .env("BOOP_NO_SYNC", "1")
            .output()
            .expect("run boop")
    }

    /// The last integer a `boop db --format text` table prints, or 0.
    fn scalar(&self, sql: &str) -> i64 {
        let out = self.boop(&["db", "--format", "text", sql]);
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| line.trim().parse::<i64>().ok())
            .last()
            .unwrap_or(0)
    }

    /// One `boop db` query as a tab-separated text table (header plus rows).
    fn query(&self, sql: &str) -> String {
        let out = self.boop(&["db", "--format", "text", sql]);
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let mail = self.mail.display().to_string();
        let _ = self.boop(&["beep", "lane", "delete", &self.lane, "--mail-dir", &mail]);
        let _ = tmux(&["kill-session", "-t", &self.lane]);
        let _ = tmux(&["kill-session", "-t", &self.coordinator_session]);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn tmux(args: &[&str]) -> std::process::Output {
    Command::new("tmux").args(args).output().expect("run tmux")
}

fn screen(session: &str) -> String {
    let output = tmux(&["capture-pane", "-p", "-t", session, "-S", "-200"]);
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo)
        .status()
        .expect("git is required by this test");
    assert!(status.success(), "git {args:?}");
}

fn head_sha(repo: &Path) -> String {
    let out = Command::new("git")
        .args(["-C", &repo.display().to_string(), "rev-parse", "HEAD"])
        .output()
        .expect("git rev-parse");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// Poll `probe` until true, or fail naming harness and step.
fn wait_for<F: FnMut() -> bool>(case: &Case, step: u8, what: &str, mut probe: F) {
    let deadline = Instant::now() + STEP_DEADLINE;
    loop {
        if probe() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{} step {step}: {what} within {:?}",
            case.entry,
            STEP_DEADLINE
        );
        std::thread::sleep(POLL);
    }
}

fn wait_for_screen(case: &Case, session: &str, wanted: &str, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let text = screen(session);
        if text.contains(wanted) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{} {label}: never saw {wanted:?}\n{text}",
            case.entry
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// The coordinator route exists and names a live session or app-server socket,
/// so the commit row has a door to land on. `boop tui` writes the route before
/// the first turn binds a session.
fn wait_for_coordinator(scratch: &Scratch, case: &Case) {
    let route = scratch.coordinator_route;
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let rows = scratch.query(&format!(
            "SELECT kind, COALESCE(session_id,''), COALESCE(app_server_socket,'') \
             FROM agent_route WHERE route = '{route}'"
        ));
        let bound = rows.lines().any(|line| {
            let mut cells = line.split('\t');
            let kind = cells.next().unwrap_or("");
            let session = cells.next().unwrap_or("");
            let socket = cells.next().unwrap_or("");
            kind == "coordinator" && (!session.trim().is_empty() || !socket.trim().is_empty())
        });
        if bound {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{} step 3: coordinator route {route} never bound a session\n{rows}",
            case.entry
        );
        std::thread::sleep(POLL);
    }
}

/// The lane route is registered under the coordinator, which is the parent edge
/// `HeadWatch` reads to pick its subscribers.
fn wait_for_lane(scratch: &Scratch, case: &Case) {
    let lane = &scratch.lane;
    let route = scratch.coordinator_route;
    wait_for(case, 4, &format!("lane route {lane} under {route}"), || {
        scratch
            .query(&format!(
                "SELECT kind FROM agent_route WHERE route = '{lane}' AND parent = '{route}'"
            ))
            .contains("lane")
    });
}

/// One pane, one wrapped real TUI against the loopback provider. The recipe env
/// rides an `env` prefix so the wrapper and its harness child both see the
/// scratch home and the mock port; the scratch store rides BOOP_DB.
fn run_tui_in_pane(scratch: &Scratch, case: &Case, launch: &MockTuiLaunch, workspace: &Path) {
    let tag = &scratch.coordinator_session;
    let mut command = String::from("exec env");
    for (key, value) in &launch.env {
        command.push_str(&format!(" {}={}", key, shell_quote(value)));
    }
    command.push_str(&format!(
        " {}={}",
        "BOOP_DB",
        shell_quote(
            &scratch
                .root
                .join("mail")
                .join("boop.db")
                .display()
                .to_string()
        )
    ));
    command.push_str(&format!(" {}={}", "BOOP_NO_SYNC", shell_quote("1")));
    command.push_str(&format!(
        " {} tui {} --name {} --bin {} --cwd {} --mail-dir {} --",
        shell_quote(BOOP),
        case.entry,
        scratch.coordinator_route,
        shell_quote(&launch.executable),
        shell_quote(&workspace.display().to_string()),
        shell_quote(&scratch.mail.display().to_string()),
    ));
    for arg in &launch.args {
        command.push(' ');
        command.push_str(&shell_quote(arg));
    }
    let output = tmux(&[
        "new-session",
        "-d",
        "-x",
        "120",
        "-y",
        "35",
        "-s",
        tag,
        &command,
    ]);
    assert!(
        output.status.success(),
        "{} step 3: tmux new-session failed: {}",
        case.entry,
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Build the lane create command. The lane always runs harness `claude` in
/// direct stream-json mode (`--bin`), pointed at the same llmock through the
/// mock recipe's env. `BOOP_READER_HOME` points the supervisor's claude door at
/// the coordinator's session registry; PATH carries this worktree's boop onto
/// the pane so `nice -n 10 boop beep lane run` is the binary under test.
fn lane_create_command(
    scratch: &Scratch,
    case: &Case,
    claude_bin: &Path,
    lane_launch: &MockTuiLaunch,
    base_sha: &str,
) -> Command {
    use boop_store::testing::BoopCommandExt;
    let mut env: BTreeMap<String, String> = lane_launch.env.iter().cloned().collect();
    let boop_dir = Path::new(BOOP).parent().unwrap().display().to_string();
    let path = std::env::var("PATH").unwrap_or_default();
    env.insert("PATH".into(), format!("{boop_dir}:{path}"));
    env.insert(
        "BOOP_READER_HOME".into(),
        scratch.root.join("home").display().to_string(),
    );

    let brief = scratch.repo.join("brief.md");
    let mut command = Command::new(BOOP);
    command
        .boop_test_root(&scratch.root)
        .env("BOOP_DB", scratch.root.join("mail").join("boop.db"))
        .env("BOOP_NO_SYNC", "1")
        .args(["beep", "lane", "create"])
        .arg("--branch")
        .arg(format!("feature/e2e-{}", case.entry))
        .arg("--cwd")
        .arg(&scratch.repo)
        .arg("--brief")
        .arg(&brief)
        .arg("--base-sha")
        .arg(base_sha)
        .args(["--harness", "claude", "--no-start", "--parent"])
        .arg(scratch.coordinator_route)
        .arg("--bin")
        .arg(claude_bin)
        .arg("--mail-dir")
        .arg(&scratch.mail);
    for (key, value) in &env {
        command.arg("--env").arg(format!("{key}={value}"));
    }
    command
}

/// One harness end to end. Prints `pass <entry>` on success; returns Err on a
/// skip so the caller records it.
fn run_case(
    case: &Case,
    llmock: &Path,
    claude_bin: &Path,
    registry: &Registry,
) -> Result<(), String> {
    let root = std::env::temp_dir().join(format!(
        "boop-commitpush-{}-{}",
        case.entry,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let session = format!("boop-commitpush-{}-{}", case.entry, std::process::id());
    let _ = tmux(&["kill-session", "-t", &session]);
    std::fs::create_dir_all(root.join("mail")).unwrap();
    std::fs::create_dir_all(root.join("home")).unwrap();
    std::fs::create_dir_all(root.join("lane-home")).unwrap();
    std::fs::create_dir_all(root.join("workspace")).unwrap();
    std::fs::create_dir_all(root.join("repo")).unwrap();

    // The lane's repo: one initial commit that carries the brief.
    let repo = root.join("repo");
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.email", "boop@example.invalid"]);
    git(&repo, &["config", "user.name", "Boop E2E"]);
    std::fs::write(repo.join("brief.md"), "Commit your work and report.\n").unwrap();
    std::fs::write(repo.join("seed.txt"), "seed\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "base"]);
    let base_sha = head_sha(&repo);

    let route = match case.id {
        HarnessId::Claude => "commit-e2e-claude",
        HarnessId::Codex => "commit-e2e-codex",
        HarnessId::Opencode => "commit-e2e-opencode",
        HarnessId::Kimi => "commit-e2e-kimi",
    };
    let lane = format!("feature-e2e-{}", case.entry);
    let worktree = repo
        .join(".boop-worktrees")
        .join("feature")
        .join(format!("e2e-{}", case.entry));
    let scratch = Scratch {
        root: root.clone(),
        mail: root.join("mail"),
        repo: repo.clone(),
        coordinator_session: session.clone(),
        coordinator_route: route,
        lane: lane.clone(),
    };

    // Provider and fixture.
    let fixture = root.join("llmock.yaml");
    std::fs::write(&fixture, FIXTURE_YAML).unwrap();
    let provider = mock_tui::MockProvider::spawn(llmock, Some(&fixture))
        .map_err(|error| format!("llmock spawn: {error}"))?;

    // Coordinator recipe and TUI.
    let adapter = registry.get(case.id);
    let launch = match adapter.mock_tui_launch(&mock_tui::MockTuiContext {
        home: &root.join("home"),
        workspace: &root.join("workspace"),
        port: provider.port,
    }) {
        Ok(launch) => launch,
        Err(error) => return Err(format!("{error}")),
    };
    run_tui_in_pane(&scratch, case, &launch, &root.join("workspace"));
    if let MockTuiReplay::TypePrompt { readiness } = launch.replay {
        wait_for_screen(case, &session, readiness, "step 3");
        let _ = tmux(&["send-keys", "-t", &session, "-l", mock_tui::MOCK_PROMPT]);
        std::thread::sleep(Duration::from_millis(250));
        let _ = tmux(&["send-keys", "-t", &session, "Enter"]);
    }
    wait_for_screen(case, &session, mock_tui::MOCK_REPLY_MARKER, "step 3");
    wait_for_coordinator(&scratch, case);

    // Lane recipe: the claude adapter's mock env for a lane home and the
    // worktree it will run in.
    let lane_launch = registry
        .get(HarnessId::Claude)
        .mock_tui_launch(&mock_tui::MockTuiContext {
            home: &root.join("lane-home"),
            workspace: &worktree,
            port: provider.port,
        })
        .map_err(|error| format!("lane claude recipe: {error}"))?;

    let lane_create = lane_create_command(&scratch, case, claude_bin, &lane_launch, &base_sha)
        .output()
        .expect("run lane create");
    assert!(
        lane_create.status.success(),
        "{} step 4: lane create failed\nstdout={}\nstderr={}",
        case.entry,
        String::from_utf8_lossy(&lane_create.stdout),
        String::from_utf8_lossy(&lane_create.stderr)
    );
    wait_for_lane(&scratch, case);

    // Step 5: the test commits directly, never the model.
    let status = Command::new("git")
        .args([
            "-C",
            &worktree.display().to_string(),
            "commit",
            "--allow-empty",
            "-m",
            "e2e: first",
            "-m",
            "Boop-Status: wip",
        ])
        .status()
        .expect("git commit");
    assert!(status.success(), "{} step 5: git commit failed", case.entry);
    let first_head = head_sha(&worktree);

    // Step 6: one push row and the coordinator pane shows the push.
    let push_count = format!(
        "SELECT COUNT(*) FROM agent_commit_push WHERE lane = '{lane}' \
         AND subscriber = '{route}' AND head = '{first_head}'"
    );
    let deadline = Instant::now() + STEP_DEADLINE;
    while scratch.scalar(&push_count) != 1 {
        if Instant::now() >= deadline {
            let transitions = scratch.query(&format!(
                "SELECT t.outcome, t.detail, m.kind, m.body \
                 FROM agent_delivery_transition t \
                 JOIN agent_mail m ON m.message_id = t.message_id \
                 WHERE m.from_route = '{lane}' ORDER BY t.sequence"
            ));
            let pushes = scratch.query("SELECT * FROM agent_commit_push");
            panic!(
                "{} step 6: no agent_commit_push row within {STEP_DEADLINE:?}\n\
                 transitions:\n{transitions}\npushes:\n{pushes}\npane:\n{}",
                case.entry,
                screen(&session)
            );
        }
        std::thread::sleep(POLL);
    }
    wait_for_screen(case, &session, &format!("commit {lane}"), "step 6");

    // Step 7: a drain attempt must not double-push.
    let pushes_before = scratch.scalar(&format!(
        "SELECT COUNT(*) FROM agent_commit_push WHERE lane = '{lane}' AND subscriber = '{route}'"
    ));
    let _ = scratch.boop(&["db", "status"]);
    let pushes_after = scratch.scalar(&format!(
        "SELECT COUNT(*) FROM agent_commit_push WHERE lane = '{lane}' AND subscriber = '{route}'"
    ));
    assert!(
        pushes_before == 1 && pushes_after == 1,
        "{} step 7: commit push count moved {pushes_before} -> {pushes_after}",
        case.entry
    );
    let accepted = scratch.scalar(&format!(
        "SELECT COUNT(*) FROM agent_delivery_transition t \
         JOIN agent_mail m ON m.message_id = t.message_id \
         WHERE m.from_route = '{lane}' AND m.kind = 'commit' \
         AND t.outcome IN ('accepted-by-harness', 'held-for-turn-boundary')"
    ));
    assert_eq!(
        accepted, 1,
        "{} step 7: the same head took the door {accepted} times",
        case.entry
    );

    // Step 8: a blocked commit becomes a request and reaches the screen.
    let status = Command::new("git")
        .args([
            "-C",
            &worktree.display().to_string(),
            "commit",
            "--allow-empty",
            "-m",
            "e2e: need input",
            "-m",
            "Boop-Status: blocked",
            "-m",
            "Boop-Ask: which schema?",
        ])
        .status()
        .expect("git commit");
    assert!(
        status.success(),
        "{} step 8: blocked commit failed",
        case.entry
    );
    wait_for(case, 8, "kind=request row from the lane", || {
        scratch.scalar(&format!(
            "SELECT COUNT(*) FROM agent_mail WHERE from_route = '{lane}' \
             AND to_route = '{route}' AND kind = 'request'"
        )) >= 1
    });
    wait_for_screen(case, &session, "which schema?", "step 8");

    println!("pass {}", case.entry);
    Ok(())
}

/// Resolve the shared prerequisites, then run one harness end to end. An
/// absent executable, or `llmock`, is a skip reason rather than a failure.
fn run_one(entry: &str) -> Result<(), String> {
    let case = CASES
        .iter()
        .find(|case| case.entry == entry)
        .expect("case entry is in CASES");
    let Some(llmock) = mock_tui::resolve_llmock() else {
        return Err("no llmock (cargo install --tag v0.1.2 llmock)".to_owned());
    };
    let Some(claude_bin) = mock_tui::resolve_executable("claude", "CLAUDE_BIN") else {
        return Err("no claude executable for the lane (set CLAUDE_BIN)".to_owned());
    };
    if mock_tui::resolve_executable(case.entry, case.executable_override).is_none() {
        return Err(format!("no {} executable", case.entry));
    }
    let registry = Registry::discover();
    run_case(case, &llmock, &claude_bin, &registry)
}

/// RECEIPT. A real lane supervisor sees a commit in its worktree and pushes it
/// once through the claude coordinator's own door; a blocked commit arrives as
/// a request carrying its ask. Sabotage: dropping the commit push leaves the
/// scratch store with no `agent_commit_push` row and the coordinator pane
/// without `commit <lane>`.
#[test]
fn commit_push_reaches_claude_tui() {
    match run_one("claude") {
        Ok(()) => {}
        Err(reason) => eprintln!("skip claude: {reason}"),
    }
}

/// RECEIPT, codex coordinator. Same body as the claude case; see
/// `commit_push_reaches_claude_tui`.
#[test]
fn commit_push_reaches_codex_tui() {
    match run_one("codex") {
        Ok(()) => {}
        Err(reason) => eprintln!("skip codex: {reason}"),
    }
}

/// RECEIPT, opencode coordinator. Same body as the claude case; see
/// `commit_push_reaches_claude_tui`.
#[test]
fn commit_push_reaches_opencode_tui() {
    match run_one("opencode") {
        Ok(()) => {}
        Err(reason) => eprintln!("skip opencode: {reason}"),
    }
}

/// The kimi coordinator route binds no session and kimi has no door, so no rung
/// takes the row.
#[ignore = "kimi coordinator route binds no session and kimi has no door; no rung takes the row"]
#[test]
fn commit_push_reaches_kimi_tui() {
    match run_one("kimi") {
        Ok(()) => {}
        Err(reason) => eprintln!("skip kimi: {reason}"),
    }
}
