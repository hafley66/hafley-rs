//! A `boop tui <H>` coordinator pane whose tmux server died (the incident of
//! 2026-09-12) comes back with `boop beep lane revive <name>`: the pane runs
//! again, the route re-registers the SAME session id, and a second revive
//! refuses because the target is live. One pass per coordinator harness
//! against a loopback llmock provider.
//!
//! Skips per harness when the CLI or `llmock` is absent:
//!   cargo install --git https://github.com/larsakerlund/llmock.git \
//!     --tag v0.1.2 --locked llmock
//! Executable overrides: CODEX_BIN, CLAUDE_BIN (ccz rides this),
//! OPENCODE_BIN, LLMOCK_BIN.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use boop::harness::mock_tui::{self, MockTuiLaunch, MockTuiReplay};
use boop::harness::{shell_quote, HarnessId};
use boop::Registry;

const BOOP: &str = env!("CARGO_BIN_EXE_boop");

/// Each case spawns a tmux server, an llmock provider and two real TUIs
/// against shared machine resources; run them one at a time.
static CASE_LOCK: Mutex<()> = Mutex::new(());

/// Wait for the first pane's route, session and screen.
const START_DEADLINE: Duration = Duration::from_secs(90);
/// Poll interval under every deadline loop.
const POLL: Duration = Duration::from_millis(250);

/// Pane ids restart at `%0` with each tmux server, so the pane the route names
/// must not be `%0`: the canary that restarts the server would take that id
/// back and the dead route would read live. Two pads push it to `%2`.
const PAD_PANES: usize = 2;

/// One harness's place in the matrix.
struct Case {
    entry: &'static str,
    id: HarnessId,
    executable_override: &'static str,
}

const CASES: &[Case] = &[
    Case {
        entry: "opencode",
        id: HarnessId::Opencode,
        executable_override: "OPENCODE_BIN",
    },
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
];

/// Scratch root for one case plus its throwaway tmux server.
struct Scratch {
    server: String,
    session: String,
    route: String,
    root: PathBuf,
    env: Vec<(String, String)>,
}

impl Scratch {
    fn mail(&self) -> PathBuf {
        self.root.join("mail")
    }

    /// One `boop` call carrying the case's recipe env, so a process this
    /// command starts (a revived pane and the tmux server under it) reaches
    /// the loopback provider and the scratch home, never a real account.
    fn boop(&self, args: &[&str]) -> Output {
        use boop_store::testing::BoopCommandExt;
        Command::new(BOOP)
            .args(args)
            .envs(self.env.iter().cloned())
            .boop_test_root(&self.root)
            .env("BOOP_READER_HOME", self.root.join("home"))
            .env("BOOP_DB", self.mail().join("boop.db"))
            .env("BOOP_NO_SYNC", "1")
            .output()
            .expect("run boop")
    }

    /// The same call with an answer waiting on stdin, for the `--dead` prompt.
    fn boop_answering(&self, args: &[&str], answer: &str) -> Output {
        use boop_store::testing::BoopCommandExt;
        use std::io::Write;
        use std::process::Stdio;
        let mut child = Command::new(BOOP)
            .args(args)
            .envs(self.env.iter().cloned())
            .boop_test_root(&self.root)
            .env("BOOP_READER_HOME", self.root.join("home"))
            .env("BOOP_DB", self.mail().join("boop.db"))
            .env("BOOP_NO_SYNC", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("run boop");
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(answer.as_bytes())
            .expect("write the answer");
        child.wait_with_output().expect("collect boop")
    }

    /// The `--dead` verb's arguments, minus what each case adds.
    fn revive_args<'a>(&'a self, extra: &[&'a str]) -> Vec<String> {
        let mut args = vec![
            "beep".to_owned(),
            "lane".to_owned(),
            "revive".to_owned(),
            "--socket".to_owned(),
            self.server.clone(),
            "--mail-dir".to_owned(),
            self.mail().display().to_string(),
        ];
        args.extend(extra.iter().map(|value| (*value).to_owned()));
        args
    }

    /// The one value a single-row query answers; `db --format text` may print a
    /// column header above it, so the value is the last line.
    fn scalar(&self, sql: &str) -> String {
        let out = self.boop(&["db", "--format", "text", sql]);
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .next_back()
            .unwrap_or_default()
            .trim()
            .to_owned()
    }

    fn session_id(&self) -> String {
        self.scalar(&format!(
            "SELECT COALESCE(session_id,'') FROM agent_route WHERE route = '{}'",
            self.route
        ))
    }

    /// The revived pane is still running by design, so teardown kills it.
    fn teardown(&self) {
        for pid in self.scratch_processes() {
            let _ = Command::new("kill")
                .args(["-KILL", &format!("-{pid}")])
                .output();
            let _ = Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .output();
        }
    }

    /// Every process whose cwd or `HOME` sits under this case's scratch root.
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
}

impl Drop for Scratch {
    fn drop(&mut self) {
        self.teardown();
        let _ = tmux(&self.server, &[], &["kill-server"]);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Pids whose working directory sits under `root`, from one `lsof` sweep.
fn cwd_pids_under(root: &str) -> Vec<u32> {
    let Ok(output) = Command::new("lsof")
        .args(["-a", "-d", "cwd", "-Fn"])
        .output()
    else {
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
    let Ok(output) = Command::new("ps")
        .args(["-E", "-ww", "-o", "pid=,command="])
        .output()
    else {
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

/// The harness has flushed the turn once boop's own reader can project an
/// assistant turn carrying text. A pane killed before that flush has nothing
/// to quote, which is a race in this setup, not the defect under test.
fn wait_for_recorded_turn(scratch: &Scratch, case: &Case, session: &str) {
    let deadline = Instant::now() + START_DEADLINE;
    loop {
        let _ = scratch.boop(&[
            "db",
            "sync",
            "create",
            "--mail-dir",
            &scratch.mail().display().to_string(),
        ]);
        let with_text = scratch.scalar(&format!(
            "SELECT COUNT(*) AS n FROM agent_turn t \
             JOIN dict_role r ON r.id = t.role_id \
             JOIN dict_session s ON s.id = t.session_id \
             WHERE s.value = '{session}' AND r.value = 'assistant' AND t.said <> ''"
        ));
        if with_text != "0" {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{}: the harness never recorded an assistant turn for {session}",
            case.entry
        );
        std::thread::sleep(POLL);
    }
}

/// An owned argument list as the borrowed one the runner takes.
fn as_args(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

/// A tmux command on this case's throwaway server. The env matters on the
/// call that STARTS the server: every later pane inherits it.
fn tmux(server: &str, env: &[(String, String)], args: &[&str]) -> Output {
    let mut full = vec!["-L", server];
    full.extend_from_slice(args);
    Command::new("tmux")
        .args(&full)
        .envs(env.iter().cloned())
        .output()
        .expect("run tmux")
}

fn screen(server: &str, session: &str) -> String {
    let output = tmux(
        server,
        &[],
        &["capture-pane", "-p", "-t", session, "-S", "-200"],
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn session_alive(server: &str, session: &str) -> bool {
    tmux(server, &[], &["has-session", "-t", session])
        .status
        .success()
}

fn wait_for_screen(server: &str, case: &Case, session: &str, wanted: &str, label: &str) {
    let deadline = Instant::now() + START_DEADLINE;
    loop {
        let text = screen(server, session);
        if text.contains(wanted) {
            return;
        }
        // A harness that exits at once takes its window and session with it, so
        // waiting out the deadline on an empty capture says nothing. Name it now.
        assert!(
            session_alive(server, session),
            "{} {label}: session {session} is gone, so the harness exited before \
             printing {wanted:?}. The executable resolved from PATH may not run \
             under the scratch HOME this test sets; name a real binary in the \
             harness's *_BIN variable and re-run.",
            case.entry
        );
        assert!(
            Instant::now() < deadline,
            "{} {label}: never saw {wanted:?} in {} bytes of screen:\n{text}",
            case.entry,
            text.len()
        );
        std::thread::sleep(POLL);
    }
}

/// Poll `probe` until it answers Some, or fail naming the harness and step.
fn wait_for<T, F: FnMut() -> Option<T>>(case: &Case, label: &str, mut probe: F) -> T {
    let deadline = Instant::now() + START_DEADLINE;
    loop {
        if let Some(value) = probe() {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "{}: {label} did not happen within {START_DEADLINE:?}",
            case.entry
        );
        std::thread::sleep(POLL);
    }
}

/// One pane, one wrapped real TUI against the loopback provider, exactly as
/// `tui_sigint_e2e` spells it.
fn run_tui_in_pane(scratch: &Scratch, case: &Case, launch: &MockTuiLaunch, workspace: &PathBuf) {
    let mut command = String::from("exec env");
    for (key, value) in &launch.env {
        command.push_str(&format!(" {}={}", key, shell_quote(value)));
    }
    command.push_str(&format!(
        " BOOP_DB={} BOOP_NO_SYNC={}",
        shell_quote(&scratch.mail().join("boop.db").display().to_string()),
        shell_quote("1"),
    ));
    command.push_str(&format!(
        " {} tui {} --name {} --bin {} --cwd {} --mail-dir {} --",
        shell_quote(BOOP),
        case.entry,
        scratch.route,
        shell_quote(&launch.executable),
        shell_quote(&workspace.display().to_string()),
        shell_quote(&scratch.mail().display().to_string()),
    ));
    for arg in &launch.args {
        command.push(' ');
        command.push_str(&shell_quote(arg));
    }
    let output = tmux(
        &scratch.server,
        &launch.env,
        &[
            "new-session",
            "-d",
            "-x",
            "200",
            "-y",
            "50",
            "-s",
            &scratch.session,
            &command,
        ],
    );
    assert!(
        output.status.success(),
        "{}: tmux new-session failed: {}",
        case.entry,
        String::from_utf8_lossy(&output.stderr)
    );
}

/// One harness end to end. `Err` is a skip reason.
fn run_case(case: &Case, llmock: &std::path::Path, registry: &Registry) -> Result<(), String> {
    let stamp = format!("{}-{}", case.entry, std::process::id());
    let root = std::env::temp_dir().join(format!("boop-revive-{stamp}"));
    let server = format!("boop-revive-{stamp}");
    let _ = std::fs::remove_dir_all(&root);
    let _ = tmux(&server, &[], &["kill-server"]);
    for dir in ["mail", "home", "workspace"] {
        std::fs::create_dir_all(root.join(dir)).map_err(|error| format!("mkdir {dir}: {error}"))?;
    }
    let provider = mock_tui::MockProvider::spawn(llmock, None)
        .map_err(|error| format!("llmock spawn: {error}"))?;
    let adapter = registry.get(case.id);
    let launch = adapter
        .mock_tui_launch(&mock_tui::MockTuiContext {
            home: &root.join("home"),
            workspace: &root.join("workspace"),
            port: provider.port,
        })
        .map_err(|error| format!("{error}"))?;
    let workspace = root.join("workspace");
    let scratch = Scratch {
        server: server.clone(),
        session: format!("revive-e2e-{}-pane", case.entry),
        route: format!("revive-e2e-{}", case.entry),
        root: root.clone(),
        env: launch.env.clone(),
    };

    // Step 1: a real coordinator pane on its own conversation.
    for pad in 0..PAD_PANES {
        let name = format!("pad-{pad}");
        let output = tmux(
            &server,
            &launch.env,
            &["new-session", "-d", "-s", &name, "sleep 100000"],
        );
        assert!(
            output.status.success(),
            "{}: pad pane {name}: {}",
            case.entry,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    run_tui_in_pane(&scratch, case, &launch, &workspace);
    if let MockTuiReplay::TypePrompt { readiness } = launch.replay {
        wait_for_screen(&server, case, &scratch.session, readiness, "readiness");
        let _ = tmux(
            &server,
            &[],
            &[
                "send-keys",
                "-t",
                &scratch.session,
                "-l",
                mock_tui::MOCK_PROMPT,
            ],
        );
        std::thread::sleep(Duration::from_millis(250));
        let _ = tmux(
            &server,
            &[],
            &["send-keys", "-t", &scratch.session, "Enter"],
        );
    }
    wait_for_screen(
        &server,
        case,
        &scratch.session,
        mock_tui::MOCK_REPLY_MARKER,
        "reply",
    );
    let session_id = wait_for(case, "route session id", || {
        let id = scratch.session_id();
        (!id.is_empty()).then_some(id)
    });
    let pane = scratch.scalar(&format!(
        "SELECT COALESCE(tmux,'') FROM agent_route WHERE route = '{}'",
        scratch.route
    ));
    assert_ne!(
        pane, "%0",
        "{}: the pad panes did not shift the coordinator pane id",
        case.entry
    );

    // The harness writes its own store after the screen shows the reply. A pane
    // killed before that flush has no turn to quote, which is a setup race.
    wait_for_recorded_turn(&scratch, case, &session_id);

    // Step 2: the incident. The whole tmux server dies mid-conversation, and
    // the harness processes under it die with it.
    let _ = tmux(&server, &[], &["kill-server"]);
    scratch.teardown();
    let restart = tmux(
        &server,
        &launch.env,
        &["new-session", "-d", "-s", "canary", "sleep 100000"],
    );
    assert!(
        restart.status.success(),
        "{}: restart the server: {}",
        case.entry,
        String::from_utf8_lossy(&restart.stderr)
    );

    // Step 3: the row says the route is dead and can come back.
    let listed = scratch.boop(&[
        "beep",
        "lane",
        "list",
        "--socket",
        &server,
        "--mail-dir",
        &scratch.mail().display().to_string(),
    ]);
    let listed = String::from_utf8_lossy(&listed.stdout).into_owned();
    let row = listed
        .lines()
        .find(|line| line.contains(&scratch.route))
        .unwrap_or_else(|| panic!("{}: no row for {}\n{listed}", case.entry, scratch.route))
        .to_owned();
    assert!(
        row.starts_with("dead") && row.contains("REVIVABLE"),
        "{}: row is not a revivable dead row: {row}",
        case.entry
    );

    // Step 3b: the read path. The table names the route and quotes its own
    // conversation, and the JSON carries the same row uncut.
    let table = scratch.boop(&as_args(&scratch.revive_args(&["--list"])));
    let table_out = String::from_utf8_lossy(&table.stdout).into_owned();
    assert!(
        table.status.success(),
        "{}: --list failed: {}",
        case.entry,
        String::from_utf8_lossy(&table.stderr)
    );
    let table_row = table_out
        .lines()
        .find(|line| line.contains(&scratch.route))
        .unwrap_or_else(|| panic!("{}: no table row\n{table_out}", case.entry))
        .to_owned();
    assert!(
        table_row.contains(mock_tui::MOCK_PROMPT)
            && table_row.contains(mock_tui::MOCK_REPLY_MARKER),
        "{}: table row quotes neither side of the turn: {table_row}",
        case.entry
    );
    // The window is a filter, not a decoration: a zero window offers nothing,
    // because every candidate's last activity is already in the past.
    let narrow = scratch.boop(&as_args(&scratch.revive_args(&["--list", "--since", "0s"])));
    let narrow_out = String::from_utf8_lossy(&narrow.stdout).into_owned();
    assert!(
        !narrow_out.contains(&scratch.route),
        "{}: --since 0s still offers the route: {narrow_out}",
        case.entry
    );
    let json = scratch.boop(&as_args(&scratch.revive_args(&["--list", "--json"])));
    let rows: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap_or_else(|error| {
        panic!(
            "{}: --list --json is not JSON: {error}\n{}",
            case.entry,
            String::from_utf8_lossy(&json.stdout)
        )
    });
    let row = rows
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["name"] == serde_json::json!(scratch.route))
        })
        .unwrap_or_else(|| panic!("{}: no JSON row for the route: {rows}", case.entry))
        .clone();
    assert_eq!(row["harness"], serde_json::json!(case.entry));
    assert_eq!(row["session_id"], serde_json::json!(session_id));
    assert_eq!(
        row["cwd"],
        serde_json::json!(workspace.display().to_string())
    );
    assert!(
        row["started"]
            .as_str()
            .is_some_and(|said| said.contains(mock_tui::MOCK_PROMPT)),
        "{}: JSON started cell is {:?}",
        case.entry,
        row["started"]
    );
    assert!(
        row["last_bot"]
            .as_str()
            .is_some_and(|said| said.contains(mock_tui::MOCK_REPLY_MARKER)),
        "{}: JSON last_bot cell is {:?}",
        case.entry,
        row["last_bot"]
    );

    // Step 4a: the prompt answered `none` spawns nothing.
    let declined = scratch.boop_answering(&as_args(&scratch.revive_args(&["--dead"])), "none\n");
    let declined_out = String::from_utf8_lossy(&declined.stdout).into_owned();
    assert!(
        declined.status.success() && declined_out.contains("nothing revived"),
        "{}: a declined prompt answered {declined_out:?} / {:?}",
        case.entry,
        String::from_utf8_lossy(&declined.stderr)
    );
    assert!(
        !session_alive(&server, &scratch.route),
        "{}: `none` spawned a pane anyway",
        case.entry
    );

    // Step 4b: the revive itself.
    let revived = scratch.boop(&as_args(&scratch.revive_args(&["--dead", "--yes"])));
    let revived_out = String::from_utf8_lossy(&revived.stdout).into_owned();
    assert!(
        revived.status.success(),
        "{}: revive failed: {revived_out}{}",
        case.entry,
        String::from_utf8_lossy(&revived.stderr),
    );
    assert!(
        revived_out.contains(&format!("revived {} pane ", scratch.route)),
        "{}: revive printed {revived_out:?}",
        case.entry
    );
    assert!(
        session_alive(&server, &scratch.route),
        "{}: revive left no live pane",
        case.entry
    );
    assert_eq!(
        scratch.session_id(),
        session_id,
        "{}: the revived pane rebound a different session",
        case.entry
    );
    // The resumed conversation, on screen: its opening user turn is drawn by
    // the revived TUI itself, from the transcript the old pane wrote.
    wait_for_screen(
        &server,
        case,
        &scratch.route,
        mock_tui::MOCK_PROMPT,
        "revived screen",
    );

    // Step 5: a live target refuses a second revive.
    let again = scratch.boop(&as_args(&scratch.revive_args(&[&scratch.route])));
    let message = String::from_utf8_lossy(&again.stderr).into_owned();
    assert!(
        !again.status.success() && message.contains("is live at tmux target"),
        "{}: a second revive answered {:?} / {message:?}",
        case.entry,
        String::from_utf8_lossy(&again.stdout)
    );
    scratch.teardown();
    Ok(())
}

/// Resolve the shared prerequisites, then run one harness. Absent executable or
/// llmock is a skip reason, not a failure.
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

/// RECEIPT. An opencode coordinator pane killed by a tmux server death comes
/// back on its own session and refuses a second revive. Sabotage: dropping the
/// session id from the revive command opens a fresh conversation, and step 4's
/// session_id assertion fails.
#[test]
fn a_dead_coordinator_pane_revives_on_its_session_opencode() {
    let _case = CASE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    match run_one("opencode") {
        Ok(()) => println!("pass opencode"),
        Err(reason) => println!("skip opencode: {reason}"),
    }
}

/// RECEIPT, claude pane. Same body as the opencode case; see
/// `a_dead_coordinator_pane_revives_on_its_session_opencode`.
#[test]
fn a_dead_coordinator_pane_revives_on_its_session_claude() {
    let _case = CASE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    match run_one("claude") {
        Ok(()) => println!("pass claude"),
        Err(reason) => println!("skip claude: {reason}"),
    }
}

/// RECEIPT, codex pane. Same body as the opencode case; see
/// `a_dead_coordinator_pane_revives_on_its_session_opencode`.
#[test]
fn a_dead_coordinator_pane_revives_on_its_session_codex() {
    let _case = CASE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    match run_one("codex") {
        Ok(()) => println!("pass codex"),
        Err(reason) => println!("skip codex: {reason}"),
    }
}
