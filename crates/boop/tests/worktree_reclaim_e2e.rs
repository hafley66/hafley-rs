//! Worktree and target-dir reclaim end to end: the real boop binary, a real
//! lane supervisor running a real harness against a loopback llmock provider
//! through the mock recipe, and a live coordinator TUI as the parent. One pass
//! per harness (claude, codex, opencode) with lane and coordinator both on that
//! harness.
//!
//! Cases, per harness:
//!   1 placement + reclaim: a lane's target dir is created during the run and
//!     gone within 10 s of the result row.
//!   2 merged delete: `lane delete --merged-into` removes a merged worktree,
//!     branch and target; an unmerged lane keeps its worktree.
//!   3 disk floor: a huge floor evicts retired lanes' targets oldest-first,
//!     then `lane create` exits non-zero with no route written.
//!   4 running-lane alarm: a parked lane under a huge floor mails exactly one
//!     `disk-low` row and no second within 5 s.
//!
//! Skips a harness when its executable or `llmock` is absent, exactly like
//! `commit_push_e2e.rs`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use boop::harness::mock_tui::{self, MockTuiLaunch, MockTuiReplay};
use boop::harness::{shell_quote, HarnessId};
use boop::Registry;

const BOOP: &str = env!("CARGO_BIN_EXE_boop");

const STEP_DEADLINE: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(250);

/// A real lane is heavyweight; run one harness at a time.
static LANE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn lane_lock() -> std::sync::MutexGuard<'static, ()> {
    LANE_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

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
];

/// One harness's scratch world.
struct Scratch {
    root: PathBuf,
    mail: PathBuf,
    repo: PathBuf,
    target_root: PathBuf,
    coordinator_session: String,
    coordinator_route: &'static str,
    /// Every lane session this run spawned, killed on drop even if a case
    /// failed before its delete.
    lanes: std::sync::Mutex<Vec<String>>,
    _provider: mock_tui::MockProvider,
    /// The provider port, for a lane recipe built after construction.
    port: u16,
    /// Scratch bin dir whose no-op `open` shadows the real one, so no harness
    /// process can raise a GUI app and steal the user's focus.
    gui_bin: PathBuf,
}

impl Scratch {
    /// One boop call with the scratch store and the scratch lane target root.
    fn boop(&self, args: &[&str]) -> std::process::Output {
        use boop_store::testing::BoopCommandExt;
        Command::new(BOOP)
            .args(args)
            .boop_test_root(&self.root)
            .env("BOOP_DB", self.mail.join("boop.db"))
            .env("BOOP_NO_SYNC", "1")
            .env("BOOP_LANE_TARGET_ROOT", &self.target_root)
            .output()
            .expect("run boop")
    }

    fn scalar(&self, sql: &str) -> i64 {
        let out = self.boop(&["db", "--format", "text", sql]);
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| line.trim().parse::<i64>().ok())
            .last()
            .unwrap_or(0)
    }

    fn query(&self, sql: &str) -> String {
        let out = self.boop(&["db", "--format", "text", sql]);
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn lane_target(&self, lane: &str) -> PathBuf {
        self.target_root.join(lane).join("target")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let mail = self.mail.display().to_string();
        let root = self.target_root.display().to_string();
        let _ = self.boop(&["beep", "lane", "prune", "--mail-dir", &mail]);
        if let Ok(lanes) = self.lanes.lock() {
            for lane in lanes.iter() {
                let _ = tmux(&["kill-session", "-t", lane]);
            }
        }
        let _ = tmux(&["kill-session", "-t", &self.coordinator_session]);
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn tmux(args: &[&str]) -> std::process::Output {
    Command::new("tmux").args(args).output().expect("run tmux")
}

/// A scratch bin dir whose no-op `open` shadows the real launcher, so a harness
/// process (or an ACP adapter under it) that tries to raise a browser exits
/// without touching the user's desktop. `BROWSER=true` is set alongside.
fn install_gui_bin(root: &Path) -> PathBuf {
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).expect("create gui bin dir");
    let open = bin.join("open");
    std::fs::write(&open, "#!/bin/sh\nexit 0\n").expect("write no-op open");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755))
        .expect("chmod no-op open");
    bin
}

/// `PATH` with the scratch gui bin first, then the existing value.
fn gui_safe_path(scratch: &Scratch) -> String {
    let path = std::env::var("PATH").unwrap_or_default();
    format!("{}:{path}", scratch.gui_bin.display())
}

fn screen(session: &str) -> String {
    let output = tmux(&["capture-pane", "-p", "-t", session, "-S", "-300"]);
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

fn git_ok(repo: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .args(args)
        .current_dir(repo)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn head_sha(repo: &Path) -> String {
    let out = Command::new("git")
        .args(["-C", &repo.display().to_string(), "rev-parse", "HEAD"])
        .output()
        .expect("git rev-parse");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn wait_for<F: FnMut() -> bool>(case: &Case, what: &str, mut probe: F) {
    let deadline = Instant::now() + STEP_DEADLINE;
    loop {
        if probe() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{}: {what} within {STEP_DEADLINE:?}",
            case.entry
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

/// The scratched coordinator TUI against the loopback provider.
fn run_coordinator_tui(scratch: &Scratch, case: &Case, launch: &MockTuiLaunch) {
    let tag = &scratch.coordinator_session;
    let mut command = String::from("exec env");
    for (key, value) in &launch.env {
        command.push_str(&format!(" {}={}", key, shell_quote(value)));
    }
    // Last assignment wins: the gui-safe PATH shadows the recipe's, so no
    // harness attempt can launch a real desktop app.
    command.push_str(&format!(
        " {}={} {}={}",
        "PATH",
        shell_quote(&gui_safe_path(scratch)),
        "BROWSER",
        shell_quote("true"),
    ));
    command.push_str(&format!(
        " {}={}",
        "BOOP_DB",
        shell_quote(&scratch.mail.join("boop.db").display().to_string())
    ));
    command.push_str(&format!(" {}={}", "BOOP_NO_SYNC", shell_quote("1")));
    command.push_str(&format!(
        " {}={}",
        "BOOP_LANE_TARGET_ROOT",
        shell_quote(&scratch.target_root.display().to_string())
    ));
    command.push_str(&format!(
        " {} tui {} --name {} --bin {} --cwd {} --mail-dir {} --",
        shell_quote(BOOP),
        case.entry,
        scratch.coordinator_route,
        shell_quote(&launch.executable),
        shell_quote(&scratch.root.join("workspace").display().to_string()),
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
        "{} coordinator tmux new-session failed: {}",
        case.entry,
        String::from_utf8_lossy(&output.stderr)
    );
}

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
            "{}: coordinator route {route} never bound a session\n{rows}",
            case.entry
        );
        std::thread::sleep(POLL);
    }
}

/// The lane's mock env pairs as `--env` values, plus the scratch PATH.
fn lane_env_pairs(scratch: &Scratch, launch: &MockTuiLaunch) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = launch.env.iter().cloned().collect();
    let boop_dir = Path::new(BOOP).parent().unwrap().display().to_string();
    let path = std::env::var("PATH").unwrap_or_default();
    env.insert(
        "PATH".into(),
        format!("{}:{boop_dir}:{path}", scratch.gui_bin.display()),
    );
    env.insert("BROWSER".into(), "true".into());
    env.insert(
        "BOOP_READER_HOME".into(),
        scratch.root.join("home").display().to_string(),
    );
    env
}

/// Build the lane create command for harness `case`, using that harness's own
/// mock recipe. claude names its real binary (`--bin`) so the lane takes the
/// direct stream-json channel; codex and opencode ride their ACP adapter.
fn lane_create(
    scratch: &Scratch,
    case: &Case,
    launch: &MockTuiLaunch,
    branch: &str,
    lane: &str,
    base_sha: &str,
    extra_env: &[(&str, &str)],
    extra_args: &[&str],
) -> Command {
    use boop_store::testing::BoopCommandExt;
    scratch
        .lanes
        .lock()
        .expect("lanes lock")
        .push(lane.to_owned());
    let brief = scratch.repo.join("brief.md");
    let mut command = Command::new(BOOP);
    command
        .boop_test_root(&scratch.root)
        .env("BOOP_DB", scratch.mail.join("boop.db"))
        .env("BOOP_NO_SYNC", "1")
        .env("BOOP_LANE_TARGET_ROOT", &scratch.target_root)
        .args(["beep", "lane", "create"])
        .arg("--branch")
        .arg(branch)
        .arg("--lane")
        .arg(lane)
        .arg("--cwd")
        .arg(&scratch.repo)
        .arg("--brief")
        .arg(&brief)
        .arg("--base-sha")
        .arg(base_sha)
        .args(["--harness", case.entry, "--no-start", "--parent"])
        .arg(scratch.coordinator_route)
        .arg("--mail-dir")
        .arg(&scratch.mail);
    if case.id == HarnessId::Claude {
        command.arg("--bin").arg(&launch.executable);
    }
    for (key, value) in lane_env_pairs(scratch, launch) {
        command.arg("--env").arg(format!("{key}={value}"));
    }
    for (key, value) in extra_env {
        command.arg("--env").arg(format!("{key}={value}"));
    }
    command.args(extra_args);
    command
}

fn lane_recipe(scratch: &Scratch, case: &Case, worktree: &Path) -> MockTuiLaunch {
    let registry = Registry::discover();
    let home = scratch.root.join(format!("lane-home-{}", case.entry));
    std::fs::create_dir_all(&home).expect("create lane home");
    registry
        .get(case.id)
        .mock_tui_launch(&mock_tui::MockTuiContext {
            home: &home,
            workspace: worktree,
            port: scratch.port,
        })
        .unwrap_or_else(|error| panic!("{} lane recipe: {error}", case.entry))
}

fn wait_for_result(scratch: &Scratch, case: &Case, lane: &str) {
    wait_for(case, &format!("result row for {lane}"), || {
        scratch.scalar(&format!(
            "SELECT COUNT(*) FROM agent_mail WHERE from_route = '{lane}' AND kind = 'result'"
        )) >= 1
    });
}

/// A branch/lane/worktree stem unique to this test process, so a leftover tmux
/// session from an interrupted run never collides with a fresh spawn.
fn uniq(case: &Case, tag: &str) -> String {
    format!("{}-{}-{}", tag, case.entry, std::process::id())
}

/// Case 1: the lane's target dir exists during the run and is reclaimed within
/// 10 s of its result row.
fn case_placement_and_reclaim(scratch: &Scratch, case: &Case, base_sha: &str) {
    let stem = uniq(case, "reclaim");
    let branch = format!("feature/{stem}");
    let lane = format!("feature-{stem}");
    let worktree = scratch
        .repo
        .join(".boop-worktrees")
        .join("feature")
        .join(&stem);
    let launch = lane_recipe(scratch, case, &worktree);
    // The brief's build touch: the dir exists before the lane runs, so the
    // run and the retire-reclaim both see it.
    let target = scratch.lane_target(&lane);
    std::fs::create_dir_all(&target).expect("create lane target");
    std::fs::write(target.join("touched"), b"x").expect("touch target");
    let output = lane_create(
        scratch,
        case,
        &launch,
        &branch,
        &lane,
        base_sha,
        &[("BOOP_IDLE_SHUTDOWN_SECS", "2")],
        &[],
    )
    .output()
    .expect("run lane create");
    assert!(
        output.status.success(),
        "{} case 1: lane create failed\nstdout={}\nstderr={}",
        case.entry,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    wait_for_result(scratch, case, &lane);
    wait_for(
        case,
        "target reclaimed within 10 s of the result row",
        || !target.exists(),
    );
    println!("case 1 pass {}", case.entry);
}

/// Case 2: a merged lane's delete removes worktree, branch and target; an
/// unmerged lane's delete keeps its worktree.
fn case_merged_delete(scratch: &Scratch, case: &Case, base_sha: &str) {
    // Merged lane.
    let stem = uniq(case, "merged");
    let branch = format!("feature/{stem}");
    let lane = format!("feature-{stem}");
    let worktree = scratch
        .repo
        .join(".boop-worktrees")
        .join("feature")
        .join(&stem);
    let launch = lane_recipe(scratch, case, &worktree);
    let output = lane_create(
        scratch,
        case,
        &launch,
        &branch,
        &lane,
        base_sha,
        &[("BOOP_IDLE_SHUTDOWN_SECS", "120")],
        &[],
    )
    .output()
    .expect("run lane create");
    assert!(
        output.status.success(),
        "{} case 2: lane create failed\n{}",
        case.entry,
        String::from_utf8_lossy(&output.stderr)
    );
    wait_for(case, &format!("worktree {lane}"), || worktree.exists());
    let target = scratch.lane_target(&lane);
    std::fs::create_dir_all(&target).expect("create lane target");

    // Commit in the lane and merge its branch into a scratch integration one.
    let status = Command::new("git")
        .args([
            "-C",
            &worktree.display().to_string(),
            "commit",
            "--allow-empty",
            "-m",
            "reclaim: merged",
            "-m",
            "Boop-Status: done",
        ])
        .status()
        .expect("git commit");
    assert!(
        status.success(),
        "{} case 2: lane commit failed",
        case.entry
    );
    let integration = format!("integration/{}", case.entry);
    assert!(
        git_ok(&scratch.repo, &["branch", &integration, base_sha]),
        "{} case 2: integration branch failed",
        case.entry
    );
    assert!(
        git_ok(&scratch.repo, &["checkout", &integration]),
        "{} case 2: checkout integration failed",
        case.entry
    );
    assert!(
        git_ok(&scratch.repo, &["merge", "--no-edit", &branch]),
        "{} case 2: merge failed",
        case.entry
    );

    let deleted = Command::new(BOOP)
        .args([
            "beep",
            "lane",
            "delete",
            &lane,
            "--merged-into",
            &integration,
            "--mail-dir",
            &scratch.mail.display().to_string(),
        ])
        .current_dir(&scratch.repo)
        .env("BOOP_LANE_TARGET_ROOT", &scratch.target_root)
        .env("BOOP_DB", scratch.mail.join("boop.db"))
        .env("BOOP_NO_SYNC", "1")
        .output()
        .expect("run lane delete");
    assert!(
        deleted.status.success(),
        "{} case 2: merged delete failed\n{}{}",
        case.entry,
        String::from_utf8_lossy(&deleted.stdout),
        String::from_utf8_lossy(&deleted.stderr)
    );
    wait_for(case, "merged worktree removed", || !worktree.exists());
    assert!(
        !git_ok(&scratch.repo, &["rev-parse", "--verify", "-q", &branch]),
        "{} case 2: merged branch still exists",
        case.entry
    );
    wait_for(case, "merged target removed", || !target.exists());

    // Unmerged lane: delete keeps the worktree.
    let stem2 = uniq(case, "unmerged");
    let branch2 = format!("feature/{stem2}");
    let lane2 = format!("feature-{stem2}");
    let worktree2 = scratch
        .repo
        .join(".boop-worktrees")
        .join("feature")
        .join(&stem2);
    let launch2 = lane_recipe(scratch, case, &worktree2);
    let output = lane_create(
        scratch,
        case,
        &launch2,
        &branch2,
        &lane2,
        base_sha,
        &[("BOOP_IDLE_SHUTDOWN_SECS", "120")],
        &[],
    )
    .output()
    .expect("run lane create");
    assert!(
        output.status.success(),
        "{} case 2: unmerged lane create failed\n{}",
        case.entry,
        String::from_utf8_lossy(&output.stderr)
    );
    wait_for(case, &format!("worktree {lane2}"), || worktree2.exists());
    // A commit the integration branch does not hold, so the branch is unmerged.
    let status = Command::new("git")
        .args([
            "-C",
            &worktree2.display().to_string(),
            "commit",
            "--allow-empty",
            "-m",
            "reclaim: unmerged",
            "-m",
            "Boop-Status: wip",
        ])
        .status()
        .expect("git commit");
    assert!(
        status.success(),
        "{} case 2: unmerged commit failed",
        case.entry
    );
    let deleted2 = Command::new(BOOP)
        .args([
            "beep",
            "lane",
            "delete",
            &lane2,
            "--merged-into",
            &integration,
            "--mail-dir",
            &scratch.mail.display().to_string(),
        ])
        .current_dir(&scratch.repo)
        .env("BOOP_LANE_TARGET_ROOT", &scratch.target_root)
        .env("BOOP_DB", scratch.mail.join("boop.db"))
        .env("BOOP_NO_SYNC", "1")
        .output()
        .expect("run lane delete");
    let stdout = String::from_utf8_lossy(&deleted2.stdout);
    assert!(
        stdout.contains("kept worktree"),
        "{} case 2: unmerged delete did not keep the worktree\n{stdout}",
        case.entry
    );
    assert!(
        worktree2.exists(),
        "{} case 2: unmerged worktree was removed",
        case.entry
    );
    println!("case 2 pass {}", case.entry);
}

/// Case 3: a huge floor evicts two retired lanes' targets oldest-first and
/// then refuses the create, writing no route.
fn case_disk_floor(scratch: &Scratch, case: &Case, base_sha: &str) {
    let huge = "100000";
    // Two dead lane target dirs, older first. They carry no route, so they are
    // eviction candidates.
    let old_lane = uniq(case, "retired-old");
    let new_lane = uniq(case, "retired-new");
    let old_target = scratch.lane_target(&old_lane);
    let new_target = scratch.lane_target(&new_lane);
    std::fs::create_dir_all(old_target.join("debug")).unwrap();
    std::fs::write(old_target.join("debug/old"), b"x").unwrap();
    std::thread::sleep(Duration::from_millis(50));
    std::fs::create_dir_all(new_target.join("debug")).unwrap();
    std::fs::write(new_target.join("debug/new"), b"x").unwrap();

    let stem = uniq(case, "floor");
    let branch = format!("feature/{stem}");
    let lane = format!("feature-{stem}");
    let worktree = scratch
        .repo
        .join(".boop-worktrees")
        .join("feature")
        .join(&stem);
    let launch = lane_recipe(scratch, case, &worktree);
    let output = lane_create(scratch, case, &launch, &branch, &lane, base_sha, &[], &[])
        .env("BOOP_DISK_FLOOR_GB", huge)
        .output()
        .expect("run lane create");
    assert!(
        !output.status.success(),
        "{} case 3: create must fail below the floor",
        case.entry
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("free disk") || combined.contains("floor"),
        "{} case 3: refusal did not name the floor\n{combined}",
        case.entry
    );
    assert!(
        !old_target.exists() && !new_target.exists(),
        "{} case 3: both retired targets evicted",
        case.entry
    );
    // Oldest-first: the older dir's eviction line comes first.
    let old_at = stdout.find(&old_target.display().to_string());
    let new_at = stdout.find(&new_target.display().to_string());
    assert!(
        old_at.is_some() && new_at.is_some() && old_at < new_at,
        "{} case 3: eviction not oldest-first\n{stdout}",
        case.entry
    );
    // No route and no screen row for the refused lane.
    assert_eq!(
        scratch.scalar(&format!(
            "SELECT COUNT(*) FROM agent_route WHERE route = '{lane}'"
        )),
        0,
        "{} case 3: refused lane registered a route",
        case.entry
    );
    assert!(
        !screen(&scratch.coordinator_session).contains(&lane),
        "{} case 3: coordinator screen shows a spawn",
        case.entry
    );
    println!("case 3 pass {}", case.entry);
}

/// Case 4: a parked lane under a huge floor mails one `disk-low` row and no
/// second within 5 s.
fn case_running_lane_alarm(scratch: &Scratch, case: &Case, base_sha: &str) {
    let stem = uniq(case, "alarm");
    let branch = format!("feature/{stem}");
    let lane = format!("feature-{stem}");
    let worktree = scratch
        .repo
        .join(".boop-worktrees")
        .join("feature")
        .join(&stem);
    let launch = lane_recipe(scratch, case, &worktree);
    // The huge floor rides `--env` only: the create process keeps the default
    // floor so it spawns, while the supervisor's own env carries the huge one.
    let output = lane_create(
        scratch,
        case,
        &launch,
        &branch,
        &lane,
        base_sha,
        &[
            ("BOOP_DISK_FLOOR_GB", "100000"),
            ("BOOP_IDLE_SHUTDOWN_SECS", "120"),
        ],
        &[],
    )
    .output()
    .expect("run lane create");
    assert!(
        output.status.success(),
        "{} case 4: lane create failed\n{}",
        case.entry,
        String::from_utf8_lossy(&output.stderr)
    );
    wait_for(case, &format!("worktree {lane}"), || worktree.exists());
    // The lane's own target dir is live and must survive eviction.
    let target = scratch.lane_target(&lane);
    std::fs::create_dir_all(&target).expect("create lane target");

    wait_for_screen(case, &scratch.coordinator_session, "disk-low", "case 4");
    let seen = screen(&scratch.coordinator_session);
    let count = seen.matches("disk-low").count();
    std::thread::sleep(Duration::from_secs(5));
    let after = screen(&scratch.coordinator_session)
        .matches("disk-low")
        .count();
    assert_eq!(
        count, after,
        "{} case 4: a second disk-low row arrived within 5 s ({count} -> {after})",
        case.entry
    );
    println!("case 4 pass {}", case.entry);
}

fn run_case(case: &Case, llmock: &Path) -> Result<(), String> {
    let root = std::env::temp_dir().join(format!(
        "boop-reclaim-{}-{}",
        case.entry,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let session = format!("boop-reclaim-{}-{}", case.entry, std::process::id());
    let _ = tmux(&["kill-session", "-t", &session]);
    std::fs::create_dir_all(root.join("mail")).unwrap();
    std::fs::create_dir_all(root.join("home")).unwrap();
    std::fs::create_dir_all(root.join("lane-home")).unwrap();
    std::fs::create_dir_all(root.join("workspace")).unwrap();
    std::fs::create_dir_all(root.join("repo")).unwrap();
    let gui_bin = install_gui_bin(&root);

    let repo = root.join("repo");
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.email", "boop@example.invalid"]);
    git(&repo, &["config", "user.name", "Boop E2E"]);
    std::fs::write(repo.join("brief.md"), "Finish and report.\n").unwrap();
    std::fs::write(repo.join("seed.txt"), "seed\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "base"]);
    let base_sha = head_sha(&repo);

    let route = match case.id {
        HarnessId::Claude => "reclaim-e2e-claude",
        HarnessId::Codex => "reclaim-e2e-codex",
        HarnessId::Opencode => "reclaim-e2e-opencode",
        HarnessId::Kimi => unreachable!(),
    };

    let fixture = root.join("llmock.yaml");
    std::fs::write(&fixture, FIXTURE_YAML).unwrap();
    let provider = mock_tui::MockProvider::spawn(llmock, Some(&fixture))
        .map_err(|error| format!("llmock spawn: {error}"))?;

    let registry = Registry::discover();
    let adapter = registry.get(case.id);
    let launch = adapter
        .mock_tui_launch(&mock_tui::MockTuiContext {
            home: &root.join("home"),
            workspace: &root.join("workspace"),
            port: provider.port,
        })
        .map_err(|error| format!("{error}"))?;

    let scratch = Scratch {
        root: root.clone(),
        mail: root.join("mail"),
        repo: repo.clone(),
        target_root: root.join("targets"),
        coordinator_session: session.clone(),
        coordinator_route: route,
        lanes: std::sync::Mutex::new(Vec::new()),
        port: provider.port,
        _provider: provider,
        gui_bin,
    };

    run_coordinator_tui(&scratch, case, &launch);
    if let MockTuiReplay::TypePrompt { readiness } = launch.replay {
        wait_for_screen(case, &session, readiness, "coordinator ready");
        let _ = tmux(&["send-keys", "-t", &session, "-l", mock_tui::MOCK_PROMPT]);
        std::thread::sleep(Duration::from_millis(250));
        let _ = tmux(&["send-keys", "-t", &session, "Enter"]);
    }
    wait_for_screen(
        case,
        &session,
        mock_tui::MOCK_REPLY_MARKER,
        "coordinator reply",
    );
    wait_for_coordinator(&scratch, case);

    case_placement_and_reclaim(&scratch, case, &base_sha);
    case_merged_delete(&scratch, case, &base_sha);
    case_disk_floor(&scratch, case, &base_sha);
    case_running_lane_alarm(&scratch, case, &base_sha);
    Ok(())
}

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
    run_case(case, &llmock)
}

#[test]
fn worktree_reclaim_claude() {
    let _guard = lane_lock();
    match run_one("claude") {
        Ok(()) => {}
        Err(reason) => eprintln!("skip claude: {reason}"),
    }
}

#[test]
fn worktree_reclaim_codex() {
    let _guard = lane_lock();
    match run_one("codex") {
        Ok(()) => {}
        Err(reason) => eprintln!("skip codex: {reason}"),
    }
}

#[test]
fn worktree_reclaim_opencode() {
    let _guard = lane_lock();
    match run_one("opencode") {
        Ok(()) => {}
        Err(reason) => eprintln!("skip opencode: {reason}"),
    }
}
