//! Commit-push end to end: a real lane supervisor watches a commit in its
//! worktree and the row reaches a real coordinator TUI through that harness's
//! own door, once. Every case runs harness H in both roles: the lane
//! supervisor and the coordinator TUI are H, against a loopback llmock
//! provider; the model provider is the only seam, every other part is the real
//! binary and the real TUI.
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
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use boop::harness::mock_tui::{self, MockTuiLaunch, MockTuiReplay};
use boop::harness::{shell_quote, HarnessId};
use boop::Registry;

const BOOP: &str = env!("CARGO_BIN_EXE_boop");

/// Every case spawns a tmux server, an llmock provider, a lane supervisor and
/// a real coordinator TUI against shared machine resources; run them one at a
/// time.
static CASE_LOCK: Mutex<()> = Mutex::new(());

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

/// One harness's place in the matrix. Both the lane and the coordinator run
/// `id`. `lane_model` is the model the lane supervisor passes to the harness:
/// claude and opencode use their recipe's mock spelling, codex a spelling its
/// own model catalog knows, because codex folds an unknown-model warning into
/// the reply the supervisor's startup ack reads.
struct Case {
    entry: &'static str,
    id: HarnessId,
    executable_override: &'static str,
    lane_model: &'static str,
}

const CASES: &[Case] = &[
    Case {
        entry: "claude",
        id: HarnessId::Claude,
        executable_override: "CLAUDE_BIN",
        lane_model: "claude-sonnet-4-5",
    },
    Case {
        entry: "codex",
        id: HarnessId::Codex,
        executable_override: "CODEX_BIN",
        lane_model: "gpt-5.6-luna",
    },
    Case {
        entry: "opencode",
        id: HarnessId::Opencode,
        executable_override: "OPENCODE_BIN",
        lane_model: "llmock/mock-model",
    },
    Case {
        entry: "kimi",
        id: HarnessId::Kimi,
        executable_override: "KIMI_BIN",
        lane_model: "llmock/mock-model",
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

impl Scratch {
    /// Every process whose cwd or inherited `HOME` sits under this scratch
    /// root, found even when the case panicked before recording a pid. The
    /// canonicalized root matches the cwd `lsof` reports (`/private/var` for
    /// `temp_dir()`'s `/var`).
    fn scratch_processes(&self) -> Vec<u32> {
        let root = self.root.to_string_lossy().into_owned();
        let resolved = std::fs::canonicalize(&self.root)
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|_| root.clone());
        let me = std::process::id();
        let mut pids = cwd_pids_under(&resolved);
        pids.extend(home_pids_under(&root));
        pids.sort_unstable();
        pids.dedup();
        pids.retain(|pid| *pid != me);
        pids
    }

    /// Kill every process the case left under the scratch root.
    fn reap(&self) {
        for pid in self.scratch_processes() {
            let _ = Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .output();
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let mail = self.mail.display().to_string();
        let _ = self.boop(&["beep", "lane", "delete", &self.lane, "--mail-dir", &mail]);
        let _ = tmux(&["kill-session", "-t", &self.lane]);
        let _ = tmux(&["kill-session", "-t", &self.coordinator_session]);
        self.reap();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Pids whose working directory sits under `root`, from one `lsof` sweep.
fn cwd_pids_under(root: &str) -> Vec<u32> {
    let output = Command::new("lsof")
        .args(["-a", "-d", "cwd", "-Fn"])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut pids = Vec::new();
    let mut current = None;
    for line in text.lines() {
        if let Some(pid) = line.strip_prefix('p').and_then(|pid| pid.parse().ok()) {
            current = Some(pid);
        } else if let Some(path) = line.strip_prefix('n') {
            if path.starts_with(root) {
                if let Some(pid) = current {
                    pids.push(pid);
                }
            }
        }
    }
    pids
}

/// Pids whose environment `HOME` sits under `root`, from one `ps` sweep.
/// `-ww` keeps ps from cutting the line before the `HOME=` token.
fn home_pids_under(root: &str) -> Vec<u32> {
    let output = Command::new("ps")
        .args(["-E", "-ww", "-o", "pid=,command="])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (pid, rest) = line.trim().split_once(char::is_whitespace)?;
            let pid = pid.parse().ok()?;
            let home = rest
                .split_whitespace()
                .find_map(|token| token.strip_prefix("HOME="))?;
            home.starts_with(root).then_some(pid)
        })
        .collect()
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
/// `HeadWatch` reads to pick its subscribers. Its conversation id appears only
/// after the supervisor opened the harness channel and built that watch, so the
/// test's commit lands after the baseline and is read as a real HEAD move
/// rather than folded into the starting head.
fn wait_for_lane(scratch: &Scratch, case: &Case) {
    let lane = &scratch.lane;
    let route = scratch.coordinator_route;
    wait_for(
        case,
        4,
        &format!("bound lane route {lane} under {route}"),
        || {
            scratch
                .query(&format!(
                    "SELECT kind, COALESCE(session_id,'') FROM agent_route \
                 WHERE route = '{lane}' AND parent = '{route}'"
                ))
                .lines()
                .any(|line| {
                    let mut cells = line.split('\t');
                    cells.next().unwrap_or("") == "lane"
                        && !cells.next().unwrap_or("").trim().is_empty()
                })
        },
    );
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

/// Build the lane create command for harness `case.id`. `--harness` and
/// `--model` name the same harness as the coordinator, `--env` carries its
/// mock recipe (a scratch HOME, the loopback provider), and `--bin` threads
/// the recipe's resolved executable where the harness channel accepts one.
/// `BOOP_READER_HOME` points the supervisor's reader at the coordinator's
/// session registry; PATH carries this worktree's boop onto the pane so
/// `nice -n 10 boop beep lane run` is the binary under test.
fn lane_create_command(
    scratch: &Scratch,
    case: &Case,
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
        .args(["--harness", case.entry, "--no-start", "--parent"])
        .arg(scratch.coordinator_route)
        .args(["--model", case.lane_model])
        .arg("--mail-dir")
        .arg(&scratch.mail);
    // Codex's lane channel is the npx ACP adapter, which owns its program, so
    // an executable override would replace `npx` and break the row.
    if case.id != HarnessId::Codex {
        command.arg("--bin").arg(&lane_launch.executable);
    }
    for (key, value) in &env {
        command.arg("--env").arg(format!("{key}={value}"));
    }
    command
}

/// Two decoy kimi transcripts under this scratch HOME, each in another
/// worktree and older than the launch. `boop tui kimi` must still bind the
/// session it opens in its own cwd, not one of these.
fn seed_decoy_kimi_sessions(root: &Path) {
    let old = SystemTime::now() - Duration::from_secs(3600);
    for (slug, uuid, cwd) in [
        ("wd_decoy-a", "decoy-a0001", root.join("decoy-a")),
        ("wd_decoy-b", "decoy-b0001", root.join("decoy-b")),
    ] {
        std::fs::create_dir_all(&cwd).unwrap();
        let session = root
            .join("home")
            .join(".kimi-code")
            .join("sessions")
            .join(slug)
            .join(format!("session_{uuid}"));
        let agent = session.join("agents").join("main");
        std::fs::create_dir_all(&agent).unwrap();
        std::fs::write(
            agent.join("wire.jsonl"),
            "{\"type\":\"metadata\",\"protocol_version\":\"1.4\",\"created_at\":1}\n",
        )
        .unwrap();
        std::fs::write(
            session.join("state.json"),
            format!("{{\"cwd\":\"{}\"}}", cwd.display()),
        )
        .unwrap();
        for file in [agent.join("wire.jsonl"), session.join("state.json")] {
            let handle = std::fs::OpenOptions::new().write(true).open(&file).unwrap();
            handle.set_modified(old).unwrap();
        }
    }
}

/// One harness end to end. Prints `pass <entry>` on success; returns Err on a
/// skip so the caller records it.
fn run_case(case: &Case, llmock: &Path, registry: &Registry) -> Result<(), String> {
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
    if case.id == HarnessId::Kimi {
        seed_decoy_kimi_sessions(&root);
    }

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
        HarnessId::Omp => "commit-e2e-omp",
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

    // Lane recipe: the same harness's mock env for a lane home and the
    // worktree the supervisor will run in.
    let lane_launch = registry
        .get(case.id)
        .mock_tui_launch(&mock_tui::MockTuiContext {
            home: &root.join("lane-home"),
            workspace: &worktree,
            port: provider.port,
        })
        .map_err(|error| format!("lane {} recipe: {error}", case.entry))?;

    let lane_create = lane_create_command(&scratch, case, &lane_launch, &base_sha)
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
            let mail = scratch.query(
                "SELECT kind, from_route, to_route, COALESCE(detail,'') \
                 FROM agent_mail ORDER BY rowid",
            );
            panic!(
                "{} step 6: no agent_commit_push row within {STEP_DEADLINE:?}\n\
                 transitions:\n{transitions}\npushes:\n{pushes}\nmail:\n{mail}\n\
                 coordinator:\n{}\nlane:\n{}",
                case.entry,
                screen(&session),
                screen(&lane)
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
    if mock_tui::resolve_executable(case.entry, case.executable_override).is_none() {
        return Err(format!("no {} executable", case.entry));
    }
    let registry = Registry::discover();
    run_case(case, &llmock, &registry)
}

/// RECEIPT. A real claude lane supervisor sees a commit in its worktree and
/// pushes it once through a real claude coordinator TUI's own door; a blocked
/// commit arrives as a request carrying its ask. Sabotage: dropping the commit
/// push leaves the scratch store with no `agent_commit_push` row and the
/// coordinator pane without `commit <lane>`.
#[test]
fn commit_push_claude_lane_to_claude_tui() {
    let _case = CASE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    match run_one("claude") {
        Ok(()) => {}
        Err(reason) => eprintln!("skip commit_push claude: {reason}"),
    }
}

/// RECEIPT, codex lane and coordinator. Same body as the claude case; see
/// `commit_push_claude_lane_to_claude_tui`.
#[test]
fn commit_push_codex_lane_to_codex_tui() {
    let _case = CASE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    match run_one("codex") {
        Ok(()) => {}
        Err(reason) => eprintln!("skip commit_push codex: {reason}"),
    }
}

/// RECEIPT, opencode lane and coordinator. Same body as the claude case; see
/// `commit_push_claude_lane_to_claude_tui`.
#[test]
fn commit_push_opencode_lane_to_opencode_tui() {
    let _case = CASE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    match run_one("opencode") {
        Ok(()) => {}
        Err(reason) => eprintln!("skip commit_push opencode: {reason}"),
    }
}

/// RECEIPT, kimi lane and coordinator. Same body as the claude case; see
/// `commit_push_claude_lane_to_claude_tui`. The kimi coordinator registers its
/// pane as a coordinator route bound to its transcript session, and a row
/// lands by the pane-submit paste rung.
#[test]
fn commit_push_reaches_kimi_tui() {
    let _case = CASE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    match run_one("kimi") {
        Ok(()) => {}
        Err(reason) => eprintln!("skip kimi: {reason}"),
    }
}
