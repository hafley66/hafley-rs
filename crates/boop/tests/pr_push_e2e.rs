//! PR push end to end: a real lane supervisor, or transcript ingest, pushes one
//! kind=pr notice through a live coordinator TUI's own door. Every case runs
//! once per coordinator harness H in {claude, codex, opencode} as `<case>_<H>`,
//! against a loopback llmock provider; the model provider is the only seam,
//! every other part is the real binary and the real TUI. kimi is skipped: its
//! coordinator route binds no session and it has no door.
//!
//! The lane is claude for every H. Its stream-json channel is the one harness
//! channel that executes the brief's `gh pr create` under llmock and writes the
//! `pr-link` transcript record; the supervisor's `gh pr view` and transcript
//! ingest are both claude-shaped, so the PR producers do not exist for a codex
//! or opencode lane. The coordinator is the dimension under test here.
//!
//! Skips a harness when its executable, or `llmock`, is absent, printed:
//!   cargo install --git https://github.com/larsakerlund/llmock.git \
//!     --tag v0.1.2 --locked llmock
//! Executable overrides: CODEX_BIN, CLAUDE_BIN (ccz rides this),
//! OPENCODE_BIN, KIMI_BIN, LLMOCK_BIN.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use boop::bus;
use boop::harness::mock_tui::{self, MockTuiLaunch, MockTuiReplay};
use boop::harness::{shell_quote, HarnessId};
use boop::Registry;

const BOOP: &str = env!("CARGO_BIN_EXE_boop");
const PR_URL: &str = "https://github.com/a/b/pull/7";
const PR_BASE: &str = "main";

/// One step's deadline: coordinator readiness, lane completion, screen arrival.
const STEP_DEADLINE: Duration = Duration::from_secs(30);
/// The poll interval under every deadline loop.
const POLL: Duration = Duration::from_millis(250);

/// Each case spawns a tmux server, an llmock provider and a real TUI against
/// shared machine resources; run them one at a time.
static CASE_LOCK: Mutex<()> = Mutex::new(());

/// The provider fixture: the lane's readiness probe answers `boop`; the brief
/// turn (which carries the post-PR line `gh pr create --fill --base`) answers a
/// Bash `gh pr create` tool call; anything else, including the coordinator's
/// own turns, is the terminal reply.
const FIXTURE_YAML: &str = r#"rules:
  - match:
      user_contains: "Respond exactly with: boop"
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
      content: |-
        FIXED_TERMINAL_REPLY
"#;

/// One harness's place in the matrix. The coordinator TUI is `id`; the lane is
/// always claude, the one harness whose stream-json channel executes the
/// `gh pr create` tool call and writes the `pr-link` record the two producers
/// read. See the module doc.
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

fn case_for(entry: &str) -> &'static Case {
    CASES
        .iter()
        .find(|case| case.entry == entry)
        .expect("case entry is in CASES")
}

/// The producer a case drives.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// The lane supervisor reads `gh pr view` and pushes the row itself.
    Supervisor,
    /// `gh pr view` fails, so only transcript ingest sees the pr-link.
    Ingest,
    /// Both producers race the same url; one notice reaches the door.
    Both,
    /// `gh pr view` hangs; the timeout warns and nothing is pushed.
    Hung,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Supervisor => "supervisor",
            Kind::Ingest => "ingest",
            Kind::Both => "both",
            Kind::Hung => "hung",
        }
    }

    fn gh(self) -> GhView {
        match self {
            Kind::Ingest => GhView::Fails,
            Kind::Hung => GhView::Hangs,
            Kind::Supervisor | Kind::Both => GhView::Answers,
        }
    }
}

/// What the gh stub does for `pr view`.
#[derive(Clone, Copy)]
enum GhView {
    Answers,
    Fails,
    Hangs,
}

/// One scratch world: coordinator home, lane home, workspace, repo and mailbox,
/// on a throwaway tmux server.
struct Scratch {
    root: PathBuf,
    mail: PathBuf,
    home: PathBuf,
    lane_home: PathBuf,
    workspace: PathBuf,
    repo: PathBuf,
    server: String,
    session: String,
    route: String,
    lane: String,
    bin: PathBuf,
}

impl Scratch {
    fn new(case: &Case, kind: Kind) -> Scratch {
        let tag = format!("{}-{}", kind.label(), case.entry);
        let root = std::env::temp_dir().join(format!("boop-prpush-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mail = root.join("mail");
        let home = root.join("home");
        let lane_home = root.join("lane-home");
        let workspace = root.join("workspace");
        let repo = root.join("repo");
        let bin = root.join("bin");
        for dir in [&mail, &home, &lane_home, &workspace, &repo, &bin] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::create_dir_all(root.join("config/boop")).unwrap();
        std::fs::write(root.join("config/boop/config.json"), "{}").unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["config", "user.email", "boop@example.invalid"]);
        git(&repo, &["config", "user.name", "Boop E2E"]);
        std::fs::write(repo.join("seed.txt"), "seed\n").unwrap();
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-qm", "seed"]);
        let _ = std::os::unix::fs::symlink(BOOP, bin.join("boop"));
        make_executable(&bin.join("gh"), &gh_script(kind.gh()));
        write_gui_shield(&bin);
        Scratch {
            root,
            mail,
            home,
            lane_home,
            workspace,
            repo,
            server: format!("boop-prpush-{tag}-{}", std::process::id()),
            session: format!("boop-prpush-{tag}-{}", std::process::id()),
            route: format!("prpush-e2e-{tag}"),
            lane: format!("feature-prpush-{tag}"),
            bin,
        }
    }

    /// One boop query against the scratch store; the sync hatch stays on so a
    /// read never walks the whole real home.
    fn boop(&self, args: &[&str]) -> std::process::Output {
        use boop_store::testing::BoopCommandExt;
        Command::new(BOOP)
            .args(args)
            .boop_test_root(&self.root)
            .env("BOOP_DB", self.mail.join("boop.db"))
            .env("BOOP_NO_SYNC", "1")
            .output()
            .expect("run boop")
    }

    /// The last integer a `db --format text` table prints, or 0.
    fn scalar(&self, sql: &str) -> i64 {
        let out = self.boop(&["db", "--format", "text", sql]);
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| line.trim().parse::<i64>().ok())
            .last()
            .unwrap_or(0)
    }

    /// One `db --format text` query as a tab-separated table.
    fn query(&self, sql: &str) -> String {
        let out = self.boop(&["db", "--format", "text", sql]);
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// One sync pass with the lane's scratch home as the reader root, so the
    /// projection finds the lane transcript that carries the pr-link.
    fn sync(&self) {
        use boop_store::testing::BoopCommandExt;
        let out = Command::new(BOOP)
            .boop_test_root(&self.root)
            .env("BOOP_DB", self.mail.join("boop.db"))
            .env("BOOP_NO_SYNC", "1")
            .env("BOOP_READER_HOME", &self.lane_home)
            .args(["db", "sync", "create"])
            .output()
            .expect("run boop db sync create");
        assert!(
            out.status.success(),
            "db sync failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// One drain pass through the live doors. `db status` is a sync-carrying
    /// read verb, so the hatch is off here on purpose.
    fn status(&self) {
        use boop_store::testing::BoopCommandExt;
        let out = Command::new(BOOP)
            .boop_test_root(&self.root)
            .env("BOOP_DB", self.mail.join("boop.db"))
            .args(["db", "status"])
            .output()
            .expect("run boop db status");
        assert!(
            out.status.success(),
            "db status failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn tmux(&self, args: &[&str]) -> std::process::Output {
        let mut full = vec!["-L", self.server.as_str()];
        full.extend_from_slice(args);
        Command::new("tmux").args(&full).output().expect("run tmux")
    }

    fn screen(&self) -> String {
        let out = self.tmux(&["capture-pane", "-p", "-t", &self.session, "-S", "-200"]);
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// The claude session id the lane pinned, from the supervisor's trail.
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

    /// Register the lane's session against the live coordinator, the edge the
    /// lane route carried before its pane dropped on exit.
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
            parent: Some(self.route.clone()),
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

    /// Every process whose cwd or `HOME` sits under this case's scratch root.
    fn scratch_processes(&self) -> Vec<u32> {
        let root = self.root.to_string_lossy().into_owned();
        // lsof reports the resolved cwd (`/private/var/...`) while `temp_dir()`
        // names `/var/...`; the cwd sweep matches the canonical spelling.
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

    /// Kill anything this case left running.
    fn kill_survivors(&self) {
        for pid in self.scratch_processes() {
            if !pid_alive(pid) {
                continue;
            }
            // A backend leads its own process group (setsid), so the negative
            // pid reaches the server and its children.
            let _ = Command::new("kill")
                .args(["-KILL", &format!("-{pid}")])
                .output();
            let _ = Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .output();
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = self.tmux(&["kill-server"]);
        self.kill_survivors();
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

fn head_sha(repo: &Path) -> String {
    let out = Command::new("git")
        .args(["-C", &repo.display().to_string(), "rev-parse", "HEAD"])
        .output()
        .expect("git rev-parse");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn make_executable(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Create the scratch bin dir and shadow `open` with a no-op, so a harness that
/// decides to raise a browser cannot steal the operator's desktop focus.
fn write_gui_shield(bin: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(bin).expect("create gui shield bin dir");
    let open = bin.join("open");
    std::fs::write(&open, "#!/bin/sh\nexit 0\n").expect("write no-op open");
    std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755))
        .expect("make no-op open executable");
}

/// The env every harness process gets: `BROWSER=true`, and the scratch bin dir
/// first on `PATH` so its no-op `open` wins.
fn gui_shield(bin: &Path) -> Vec<(String, String)> {
    vec![
        ("BROWSER".into(), "true".into()),
        (
            "PATH".into(),
            format!(
                "{}:{}",
                bin.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        ),
    ]
}

/// The gh stub: `pr create` prints the url; `pr view` answers, fails, or hangs.
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
fn home_pids_under(root: &str) -> Vec<u32> {
    let output = Command::new("ps")
        // `-ww`: without it ps may cut the line before the `HOME=` token.
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

fn pid_alive(pid: u32) -> bool {
    let output = Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output();
    match output {
        Ok(output) => {
            let stat = String::from_utf8_lossy(&output.stdout);
            let stat = stat.trim();
            !stat.is_empty() && !stat.starts_with('Z')
        }
        Err(_) => false,
    }
}

/// One pane, one wrapped real TUI against the loopback provider. The recipe env
/// rides an `env` prefix so the wrapper and its harness child both see the
/// scratch home and the mock port; the scratch store rides BOOP_DB.
fn run_tui_in_pane(scratch: &Scratch, case: &Case, launch: &MockTuiLaunch, workspace: &Path) {
    let mut command = String::from("exec env");
    for (key, value) in &launch.env {
        command.push_str(&format!(" {}={}", key, shell_quote(value)));
    }
    for (key, value) in gui_shield(&scratch.bin) {
        command.push_str(&format!(" {}={}", key, shell_quote(&value)));
    }
    command.push_str(&format!(
        " {}={}",
        "BOOP_DB",
        shell_quote(&scratch.mail.join("boop.db").display().to_string())
    ));
    command.push_str(&format!(" {}={}", "BOOP_NO_SYNC", shell_quote("1")));
    command.push_str(&format!(
        " {} tui {} --name {} --bin {} --cwd {} --mail-dir {} --",
        shell_quote(BOOP),
        case.entry,
        scratch.route,
        shell_quote(&launch.executable),
        shell_quote(&workspace.display().to_string()),
        shell_quote(&scratch.mail.display().to_string()),
    ));
    for arg in &launch.args {
        command.push(' ');
        command.push_str(&shell_quote(arg));
    }
    let output = scratch.tmux(&[
        "new-session",
        "-d",
        "-x",
        "120",
        "-y",
        "35",
        "-s",
        &scratch.session,
        &command,
    ]);
    assert!(
        output.status.success(),
        "{} {}: tmux new-session failed: {}",
        case.entry,
        launch.executable,
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Strip whitespace and the box-drawing gutter a TUI draws at the left edge, so
/// a row the terminal wrapped or a URL it broke across lines still matches as
/// one string.
fn squeeze(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '\u{2500}'..='\u{257f}'))
        .collect()
}

fn wait_for_screen(scratch: &Scratch, case: &Case, wanted: &str, label: &str) {
    let deadline = Instant::now() + STEP_DEADLINE;
    let needle = squeeze(wanted);
    loop {
        let text = scratch.screen();
        if squeeze(&text).contains(&needle) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{} {label}: never saw {wanted:?}\n{text}",
            case.entry
        );
        std::thread::sleep(POLL);
    }
}

/// The coordinator route exists and names a live session or app-server socket,
/// so the pr row has a door to land on.
fn wait_for_coordinator(scratch: &Scratch, case: &Case) {
    let route = scratch.route.clone();
    let deadline = Instant::now() + STEP_DEADLINE;
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

/// Build the lane create command. The lane runs harness `claude` in direct
/// stream-json mode (`--bin`), pointed at the same llmock through its mock
/// recipe env. PATH carries this worktree's gh stub and boop binary onto the
/// pane so the supervisor's `gh pr view` and the lane's `gh pr create` run the
/// scratch stubs.
fn create_lane(
    scratch: &Scratch,
    case: &Case,
    kind: Kind,
    claude_bin: &Path,
    lane_launch: &MockTuiLaunch,
    extra_env: &[(String, String)],
) -> std::process::Output {
    use boop_store::testing::BoopCommandExt;
    let mut env: BTreeMap<String, String> = lane_launch.env.iter().cloned().collect();
    let boop_dir = Path::new(BOOP).parent().unwrap().display().to_string();
    let path = std::env::var("PATH").unwrap_or_default();
    env.insert(
        "PATH".into(),
        format!("{}:{boop_dir}:{path}", scratch.bin.display()),
    );
    env.insert("BROWSER".into(), "true".into());
    // The pane runs the supervisor on the scratch store and the scratch trail,
    // so the spawn record it reads (post_pr) sits beside the one create wrote.
    env.insert(
        "BOOP_DB".into(),
        scratch.mail.join("boop.db").display().to_string(),
    );
    env.insert("BOOP_NO_SYNC".into(), "1".into());
    env.insert(
        "BOOP_READER_HOME".into(),
        scratch.lane_home.display().to_string(),
    );
    for (key, value) in extra_env {
        env.insert(key.clone(), value.clone());
    }

    let brief = scratch
        .repo
        .join(format!("brief-{}-{}.md", kind.label(), case.entry));
    std::fs::write(&brief, "finish and report\n").unwrap();
    let base_sha = head_sha(&scratch.repo);
    let branch = format!("feature/prpush-{}-{}", kind.label(), case.entry);

    let mut command = Command::new(BOOP);
    command
        .boop_test_root(&scratch.root)
        .env("BOOP_DB", scratch.mail.join("boop.db"))
        .env("BOOP_NO_SYNC", "1")
        .args(["beep", "lane", "create"])
        .arg("--branch")
        .arg(&branch)
        .arg("--cwd")
        .arg(&scratch.repo)
        .arg("--brief")
        .arg(&brief)
        .arg("--base-sha")
        .arg(&base_sha)
        .args(["--harness", "claude", "--no-start", "--parent"])
        .arg(&scratch.route)
        .arg("--bin")
        .arg(claude_bin)
        .arg("--tmux")
        .arg(&scratch.lane)
        .arg("--socket")
        .arg(&scratch.server)
        .arg("--mail-dir")
        .arg(&scratch.mail)
        .args(["--post-pr", "--pr-base", PR_BASE]);
    for (key, value) in &env {
        command.arg("--env").arg(format!("{key}={value}"));
    }
    command.output().expect("run lane create")
}

fn wait_for_count(scratch: &Scratch, sql: &str, want: i64, deadline: Duration) -> bool {
    let deadline = Instant::now() + deadline;
    loop {
        if scratch.scalar(sql) >= want {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(POLL);
    }
}

fn wait_for_log(scratch: &Scratch, wanted: &str) -> bool {
    let deadline = Instant::now() + STEP_DEADLINE;
    loop {
        if scratch.supervise_log().contains(wanted) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(POLL);
    }
}

fn pr_mail_count(scratch: &Scratch) -> i64 {
    scratch.scalar("SELECT COUNT(*) AS n FROM agent_mail WHERE kind = 'pr'")
}

fn notice_count(scratch: &Scratch) -> i64 {
    scratch.scalar("SELECT COUNT(*) AS n FROM agent_pr_notice")
}

/// One producer case against one harness: bring up the live coordinator, run
/// the lane, then assert the row reached the coordinator's own door.
fn run_case(
    case: &Case,
    kind: Kind,
    llmock: &Path,
    claude_bin: &Path,
    registry: &Registry,
) -> Result<(), String> {
    let scratch = Scratch::new(case, kind);
    let fixture = scratch.root.join("llmock.yaml");
    std::fs::write(&fixture, FIXTURE_YAML).map_err(|error| error.to_string())?;
    let provider = mock_tui::MockProvider::spawn(llmock, Some(&fixture))
        .map_err(|error| format!("llmock spawn: {error}"))?;

    // Coordinator TUI, one pane.
    let launch = registry
        .get(case.id)
        .mock_tui_launch(&mock_tui::MockTuiContext {
            home: &scratch.home,
            workspace: &scratch.workspace,
            port: provider.port,
        })
        .map_err(|error| format!("{error}"))?;
    run_tui_in_pane(&scratch, case, &launch, &scratch.workspace);
    if let MockTuiReplay::TypePrompt { readiness } = launch.replay {
        wait_for_screen(&scratch, case, readiness, "coordinator readiness");
        let _ = scratch.tmux(&[
            "send-keys",
            "-t",
            &scratch.session,
            "-l",
            mock_tui::MOCK_PROMPT,
        ]);
        std::thread::sleep(Duration::from_millis(250));
        let _ = scratch.tmux(&["send-keys", "-t", &scratch.session, "Enter"]);
    }
    wait_for_screen(
        &scratch,
        case,
        mock_tui::MOCK_REPLY_MARKER,
        "coordinator reply",
    );
    wait_for_coordinator(&scratch, case);

    // Lane recipe: the claude adapter's mock env for the lane home and the
    // worktree lane create will cut.
    let worktree = scratch
        .repo
        .join(".boop-worktrees")
        .join("feature")
        .join(format!("prpush-{}-{}", kind.label(), case.entry));
    let lane_launch = registry
        .get(HarnessId::Claude)
        .mock_tui_launch(&mock_tui::MockTuiContext {
            home: &scratch.lane_home,
            workspace: &worktree,
            port: provider.port,
        })
        .map_err(|error| format!("lane claude recipe: {error}"))?;

    let extra: Vec<(String, String)> = if kind == Kind::Hung {
        vec![("BOOP_PR_VIEW_TIMEOUT_SECS".into(), "1".into())]
    } else {
        Vec::new()
    };

    let started = Instant::now();
    let created = create_lane(&scratch, case, kind, claude_bin, &lane_launch, &extra);
    assert!(
        created.status.success(),
        "{} {}: lane create failed\nstdout={}\nstderr={}",
        kind.label(),
        case.entry,
        String::from_utf8_lossy(&created.stdout),
        String::from_utf8_lossy(&created.stderr)
    );
    assert!(
        wait_for_count(
            &scratch,
            "SELECT COUNT(*) AS n FROM agent_mail WHERE kind = 'result'",
            1,
            STEP_DEADLINE,
        ),
        "{} {}: the lane never wrote a result row",
        kind.label(),
        case.entry
    );
    let elapsed = started.elapsed();

    let wanted = format!("pr {} {}", scratch.lane, PR_URL);
    match kind {
        Kind::Supervisor => {
            wait_for_screen(&scratch, case, &wanted, "pr row");
            assert_eq!(
                notice_count(&scratch),
                1,
                "{} {}: agent_pr_notice rows\n{}",
                kind.label(),
                case.entry,
                scratch.query("SELECT pr_url, lane FROM agent_pr_notice")
            );
            assert_eq!(
                pr_mail_count(&scratch),
                1,
                "{} {}: pr mail rows\n{}",
                kind.label(),
                case.entry,
                scratch.query("SELECT kind, to_route, body FROM agent_mail WHERE kind = 'pr'")
            );
        }
        Kind::Ingest => {
            assert_eq!(
                pr_mail_count(&scratch),
                0,
                "{} {}: the failing gh must not push from the supervisor",
                kind.label(),
                case.entry
            );
            let session = scratch.lane_session().expect("the lane pinned its session");
            scratch.seed_session_route(&session);
            scratch.sync();
            scratch.status();
            wait_for_screen(&scratch, case, &wanted, "pr row after ingest");
            assert_eq!(
                notice_count(&scratch),
                1,
                "{} {}: agent_pr_notice rows\n{}",
                kind.label(),
                case.entry,
                scratch.query("SELECT pr_url, lane FROM agent_pr_notice")
            );
        }
        Kind::Both => {
            assert!(
                wait_for_count(
                    &scratch,
                    "SELECT COUNT(*) AS n FROM agent_mail WHERE kind = 'pr'",
                    1,
                    STEP_DEADLINE,
                ),
                "{} {}: the supervisor pushed nothing",
                kind.label(),
                case.entry
            );
            wait_for_screen(&scratch, case, &wanted, "pr row");
            let session = scratch.lane_session().expect("the lane pinned its session");
            scratch.seed_session_route(&session);
            scratch.sync();
            scratch.status();
            std::thread::sleep(Duration::from_secs(1));
            assert_eq!(
                notice_count(&scratch),
                1,
                "{} {}: the second producer claimed the url again",
                kind.label(),
                case.entry
            );
            assert_eq!(
                pr_mail_count(&scratch),
                1,
                "{} {}: the second producer appended a duplicate notice\n{}",
                kind.label(),
                case.entry,
                scratch.query("SELECT kind, to_route, body FROM agent_mail WHERE kind = 'pr'")
            );
            let hits = squeeze(&scratch.screen())
                .matches(&squeeze(&format!("pr {} {PR_URL}", scratch.lane)))
                .count();
            assert_eq!(
                hits,
                1,
                "{} {}: the pr row reached the screen {hits} times\n{}",
                kind.label(),
                case.entry,
                scratch.screen()
            );
        }
        Kind::Hung => {
            assert!(
                elapsed < Duration::from_secs(12),
                "{} {}: the hung gh stalled the lane for {elapsed:?}",
                kind.label(),
                case.entry
            );
            assert_eq!(
                pr_mail_count(&scratch),
                0,
                "{} {}: a hung gh must not push a pr row",
                kind.label(),
                case.entry
            );
            assert!(
                wait_for_log(&scratch, "gh pr view timed out"),
                "{} {}: the timeout was not warned:\n{}",
                kind.label(),
                case.entry,
                scratch.supervise_log()
            );
            assert!(
                !scratch.screen().contains(PR_URL),
                "{} {}: the coordinator saw a PR the supervisor never read\n{}",
                kind.label(),
                case.entry,
                scratch.screen()
            );
        }
    }
    Ok(())
}

/// Resolve the shared prerequisites, then run one case for one coordinator
/// harness. An absent executable, or `llmock`, is a printed skip, never a
/// failure.
fn run_one(entry: &str, kind: Kind) -> Result<(), String> {
    let case = case_for(entry);
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
    run_case(case, kind, &llmock, &claude_bin, &registry)
}

/// One test body: hold the case lock, print the outcome for `<case>_<H>`.
fn run(entry: &str, kind: Kind) {
    let _case = CASE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    match run_one(entry, kind) {
        Ok(()) => println!("pass {} {}", kind.label(), entry),
        Err(reason) => println!("skip {} {entry}: {reason}", kind.label()),
    }
}

/// RECEIPT. `lane create --post-pr --pr-base` prints the toggle on the dry-run
/// line; absent the flag it prints `off`, and `--no-post-pr` overrides config.
#[test]
fn lane_create_dry_run_prints_the_post_pr_toggle() {
    use boop_store::testing::BoopCommandExt;
    let root = std::env::temp_dir().join(format!("boop-prpush-toggle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let repo = root.join("repo");
    let mail = root.join("mail");
    for dir in [&repo, &mail] {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::create_dir_all(root.join("config/boop")).unwrap();
    std::fs::write(root.join("config/boop/config.json"), "{}").unwrap();
    std::fs::write(mail.join("registry.json"), "{}").unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.email", "boop@example.invalid"]);
    git(&repo, &["config", "user.name", "Boop Test"]);
    std::fs::write(repo.join("seed.txt"), "seed\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "seed"]);
    let brief = root.join("brief.md");
    std::fs::write(&brief, "finish and report\n").unwrap();

    let create = |extra: &[&str]| {
        let mut command = Command::new(BOOP);
        command
            .boop_test_root(&root)
            .env("BOOP_DB", mail.join("boop.db"))
            .env("BOOP_NO_SYNC", "1")
            .args(["beep", "lane", "create"])
            .arg("--lane")
            .arg("dry-run-lane")
            .arg("--cwd")
            .arg(&repo)
            .arg("--brief")
            .arg(&brief)
            .args(["--harness", "claude", "--no-start", "--dry-run"])
            .args(extra);
        command.output().expect("run boop lane create")
    };

    let on = create(&["--post-pr", "--pr-base", "dev"]);
    let stdout = String::from_utf8_lossy(&on.stdout);
    assert!(on.status.success(), "{stdout}");
    assert!(stdout.contains("post-pr: dev"), "{stdout}");

    let off = create(&[]);
    assert!(
        String::from_utf8_lossy(&off.stdout).contains("post-pr: off"),
        "{}",
        String::from_utf8_lossy(&off.stdout)
    );

    std::fs::write(
        root.join("config/boop/config.json"),
        r#"{ "post-pr": true, "pr-base": "release" }"#,
    )
    .unwrap();
    let configured = create(&[]);
    assert!(
        String::from_utf8_lossy(&configured.stdout).contains("post-pr: release"),
        "{}",
        String::from_utf8_lossy(&configured.stdout)
    );
    let overridden = create(&["--no-post-pr"]);
    assert!(
        String::from_utf8_lossy(&overridden.stdout).contains("post-pr: off"),
        "{}",
        String::from_utf8_lossy(&overridden.stdout)
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// RECEIPT (lane supervisor producer). A real claude lane runs against llmock;
/// the model calls `gh pr create`, the supervisor reads `gh pr view` and pushes
/// one kind=pr row through the live claude coordinator's own door.
#[test]
fn supervisor_claude() {
    run("claude", Kind::Supervisor);
}

/// RECEIPT, codex coordinator. Same lane; see `supervisor_claude`.
#[test]
fn supervisor_codex() {
    run("codex", Kind::Supervisor);
}

/// RECEIPT, opencode coordinator. Same lane; see `supervisor_claude`.
#[test]
fn supervisor_opencode() {
    run("opencode", Kind::Supervisor);
}

/// RECEIPT (ingest producer). `gh pr view` fails, so only transcript ingest
/// sees the pr-link; a drain walks the held row through the live claude
/// coordinator.
#[test]
fn ingest_claude() {
    run("claude", Kind::Ingest);
}

/// RECEIPT, codex coordinator. Same lane; see `ingest_claude`.
#[test]
fn ingest_codex() {
    run("codex", Kind::Ingest);
}

/// RECEIPT, opencode coordinator. Same lane; see `ingest_claude`.
#[test]
fn ingest_opencode() {
    run("opencode", Kind::Ingest);
}

/// RECEIPT (both producers). The supervisor claims the url first; a later sync
/// of the same pr-link appends nothing, so the live claude coordinator's door
/// sees the row exactly once.
#[test]
fn both_producers_claude() {
    run("claude", Kind::Both);
}

/// RECEIPT, codex coordinator. Same lane; see `both_producers_claude`.
#[test]
fn both_producers_codex() {
    run("codex", Kind::Both);
}

/// RECEIPT, opencode coordinator. Same lane; see `both_producers_claude`.
#[test]
fn both_producers_opencode() {
    run("opencode", Kind::Both);
}

/// RECEIPT (gh hangs). A `gh pr view` that never answers is killed at the
/// deadline: the lane's turn continues, the timeout warns, and the live
/// claude coordinator receives nothing for that PR.
#[test]
fn hung_gh_claude() {
    run("claude", Kind::Hung);
}

/// RECEIPT, codex coordinator. Same lane; see `hung_gh_claude`.
#[test]
fn hung_gh_codex() {
    run("codex", Kind::Hung);
}

/// RECEIPT, opencode coordinator. Same lane; see `hung_gh_claude`.
#[test]
fn hung_gh_opencode() {
    run("opencode", Kind::Hung);
}

/// kimi has no coordinator door: its route binds no session and no rung takes a
/// PR row, so there is no `<case>_kimi`. Printed here so the skip is visible.
#[test]
fn kimi_has_no_door() {
    println!("skip all kimi: kimi coordinator route binds no session and it has no door");
}
