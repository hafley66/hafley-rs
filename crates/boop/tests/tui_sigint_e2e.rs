//! Ctrl-C in a `boop tui <H>` pane: the wrapper must survive SIGINT (the TUI
//! child shares the pane's process group and handles its own copy), and when
//! the TUI exits the wrapper must take its adapter backend down with it. One
//! pass per coordinator harness against a loopback llmock provider.
//!
//! Skips per harness when the CLI or `llmock` is absent:
//!   cargo install --git https://github.com/larsakerlund/llmock.git \
//!     --tag v0.1.2 --locked llmock
//! Executable overrides: CODEX_BIN, CLAUDE_BIN (ccz rides this),
//! OPENCODE_BIN, LLMOCK_BIN.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use boop::harness::mock_tui::{self, MockTuiLaunch, MockTuiReplay};
use boop::harness::{shell_quote, HarnessId};
use boop::Registry;

const BOOP: &str = env!("CARGO_BIN_EXE_boop");

/// The three cases each spawn a tmux server, an llmock provider and a real TUI
/// against shared machine resources; run them one at a time.
static CASE_LOCK: Mutex<()> = Mutex::new(());

/// Wait for the route to register and the backend to appear.
const START_DEADLINE: Duration = Duration::from_secs(60);
/// The wrapper must stay alive this long after the first Ctrl-C.
const SURVIVE_WINDOW: Duration = Duration::from_secs(5);
/// The wrapper and its backend must be gone this long after the TUI exits.
const EXIT_DEADLINE: Duration = Duration::from_secs(10);
/// Poll interval under every deadline loop.
const POLL: Duration = Duration::from_millis(250);

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

/// Scratch root for one case plus the process handles the teardown guard
/// kills and a throwaway tmux server.
struct Scratch {
    server: String,
    session: String,
    route: String,
    root: PathBuf,
    wrapper_pid: Option<u32>,
    backend_pids: Vec<u32>,
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

    fn scalar(&self, sql: &str) -> String {
        let out = self.boop(&["db", "--format", "text", sql]);
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// Kill anything this case left running; returns the pids it had to kill.
    fn kill_survivors(&self) -> Vec<u32> {
        let mut targets = self.backend_pids.clone();
        if let Some(pid) = self.wrapper_pid {
            targets.push(pid);
        }
        targets.extend(self.scratch_processes());
        targets.sort_unstable();
        targets.dedup();
        let mut killed = Vec::new();
        for pid in targets {
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
            killed.push(pid);
        }
        killed
    }

    /// Every process whose cwd or `HOME` sits under this case's scratch root,
    /// found even when the case panicked before recording its backend pid.
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

    fn teardown(&self, entry: &str) {
        let killed = self.kill_survivors();
        assert!(
            killed.is_empty(),
            "{entry}: teardown had to kill orphaned pids {killed:?}"
        );
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        self.kill_survivors();
        let _ = tmux(&self.server, &["kill-server"]);
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

/// A tmux command on this case's throwaway server.
fn tmux(server: &str, args: &[&str]) -> std::process::Output {
    let mut full = vec!["-L", server];
    full.extend_from_slice(args);
    Command::new("tmux").args(&full).output().expect("run tmux")
}

fn screen(server: &str, session: &str) -> String {
    let output = tmux(server, &["capture-pane", "-p", "-t", session, "-S", "-200"]);
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn session_alive(server: &str, session: &str) -> bool {
    tmux(server, &["has-session", "-t", session])
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
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn send_keys(server: &str, session: &str, key: &str) {
    let _ = tmux(server, &["send-keys", "-t", session, key]);
}

/// The one pid the pane's process holds: `exec env ... boop tui` replaces the
/// pane shell with the wrapper.
fn pane_pid(server: &str, session: &str) -> u32 {
    let output = tmux(server, &["list-panes", "-t", session, "-F", "#{pane_pid}"]);
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("pane pid")
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

/// Deliver the wrapper's own copy of a pane Ctrl-C. A pane Ctrl-C signals the
/// whole foreground group, but a harness TUI holds the terminal raw, so the
/// `C-c` byte never raises the signal from the tty and the test cannot lean on
/// `send-keys`; signalling just the wrapper isolates its SIGINT handling from
/// the TUI's, which the exit step drives through the keyboard.
fn sigint_pid(pid: u32) {
    let _ = Command::new("kill")
        .args(["-INT", &pid.to_string()])
        .output();
}

/// A snapshot of `pid ppid command` for the descendant search.
fn process_table() -> Vec<(u32, u32, String)> {
    let output = Command::new("ps")
        .args(["-eo", "pid=,ppid=,command="])
        .output()
        .expect("run ps");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.trim().splitn(3, char::is_whitespace);
            let pid = fields.next()?.parse().ok()?;
            let ppid = fields.next()?.parse().ok()?;
            let command = fields.next().unwrap_or("").to_owned();
            Some((pid, ppid, command))
        })
        .collect()
}

fn descendants(root: u32) -> Vec<u32> {
    let table = process_table();
    let mut found = vec![root];
    let mut index = 0;
    while index < found.len() {
        let parent = found[index];
        for (pid, ppid, _) in &table {
            if *ppid == parent && !found.contains(pid) {
                found.push(*pid);
            }
        }
        index += 1;
    }
    found.retain(|pid| *pid != root);
    found
}

/// The `opencode serve` pid behind this wrapper, found as a descendant or by
/// the port the route recorded.
fn find_backend_pid(scratch: &Scratch, wrapper: u32) -> Option<u32> {
    let table = process_table();
    let descendant_set = descendants(wrapper);
    if let Some(pid) = table
        .iter()
        .find(|(pid, _, command)| {
            descendant_set.contains(pid) && command.split_whitespace().any(|token| token == "serve")
        })
        .map(|(pid, _, _)| *pid)
    {
        return Some(pid);
    }
    let socket = scratch.scalar(&format!(
        "SELECT COALESCE(app_server_socket,'') FROM agent_route WHERE route = '{}'",
        scratch.route
    ));
    let port = socket
        .rsplit(':')
        .next()
        .and_then(|tail| tail.trim_end_matches('/').parse::<u16>().ok())?;
    table
        .iter()
        .find(|(_, _, command)| command.contains(&port.to_string()) && command.contains("serve"))
        .map(|(pid, _, _)| *pid)
}

/// Poll `probe` until Some, or fail naming the harness and step.
fn wait_for<F: FnMut() -> Option<u32>>(
    case: &Case,
    label: &str,
    deadline: Duration,
    mut probe: F,
) -> u32 {
    let deadline = Instant::now() + deadline;
    loop {
        if let Some(value) = probe() {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "{}: {label} did not happen within {deadline:?}",
            case.entry
        );
        std::thread::sleep(POLL);
    }
}

/// The coordinator route is registered under this case's scratch store.
fn wait_for_route(scratch: &Scratch, case: &Case) {
    let route = scratch.route.clone();
    wait_for(case, "route registration", START_DEADLINE, || {
        let kinds = scratch.scalar(&format!(
            "SELECT kind FROM agent_route WHERE route = '{route}'"
        ));
        kinds.contains("coordinator").then_some(0)
    });
}

/// One pane, one wrapped real TUI against the loopback provider. The recipe env
/// rides an `env` prefix so the wrapper and its harness child both see the
/// scratch home and the mock port; the scratch store rides BOOP_DB.
fn run_tui_in_pane(scratch: &Scratch, case: &Case, launch: &MockTuiLaunch, workspace: &PathBuf) {
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
        scratch.route,
        shell_quote(&launch.executable),
        shell_quote(&workspace.display().to_string()),
        shell_quote(&scratch.root.join("mail").display().to_string()),
    ));
    for arg in &launch.args {
        command.push(' ');
        command.push_str(&shell_quote(arg));
    }
    let output = tmux(
        &scratch.server,
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
    let root =
        std::env::temp_dir().join(format!("boop-sigint-{}-{}", case.entry, std::process::id()));
    let server = format!("boop-sigint-{}-{}", case.entry, std::process::id());
    let session = format!("boop-sigint-{}-{}", case.entry, std::process::id());
    let route = format!("sigint-e2e-{}", case.entry);
    let _ = std::fs::remove_dir_all(&root);
    let _ = tmux(&server, &["kill-server"]);
    for dir in ["mail", "home", "workspace"] {
        std::fs::create_dir_all(root.join(dir)).map_err(|error| format!("mkdir {dir}: {error}"))?;
    }

    let provider = mock_tui::MockProvider::spawn(llmock, None)
        .map_err(|error| format!("llmock spawn: {error}"))?;
    let adapter = registry.get(case.id);
    let launch = match adapter.mock_tui_launch(&mock_tui::MockTuiContext {
        home: &root.join("home"),
        workspace: &root.join("workspace"),
        port: provider.port,
    }) {
        Ok(launch) => launch,
        Err(error) => return Err(format!("{error}")),
    };
    let workspace = root.join("workspace");
    let scratch = Scratch {
        server: server.clone(),
        session: session.clone(),
        route: route.clone(),
        root: root.clone(),
        wrapper_pid: None,
        backend_pids: Vec::new(),
    };
    run_tui_in_pane(&scratch, case, &launch, &workspace);
    let mut scratch = scratch;
    let wrapper = wait_for(case, "pane pid", START_DEADLINE, || {
        Some(pane_pid(&server, &session))
    });
    scratch.wrapper_pid = Some(wrapper);

    if let MockTuiReplay::TypePrompt { readiness } = launch.replay {
        wait_for_screen(&server, case, &session, readiness, "readiness");
        let _ = tmux(
            &server,
            &["send-keys", "-t", &session, "-l", mock_tui::MOCK_PROMPT],
        );
        std::thread::sleep(Duration::from_millis(250));
        send_keys(&server, &session, "Enter");
    }
    wait_for_screen(
        &server,
        case,
        &session,
        mock_tui::MOCK_REPLY_MARKER,
        "reply",
    );
    wait_for_route(&scratch, case);

    if case.id == HarnessId::Opencode {
        let backend = wait_for(case, "opencode serve backend", START_DEADLINE, || {
            find_backend_pid(&scratch, wrapper)
        });
        scratch.backend_pids.push(backend);
    }

    // Step 3: deliver the wrapper's copy of the pane Ctrl-C. Registering
    // SIGINT means it never kills the wrapper, which must stay up until the
    // TUI exits.
    sigint_pid(wrapper);
    let deadline = Instant::now() + SURVIVE_WINDOW;
    while Instant::now() < deadline {
        assert!(
            pid_alive(wrapper),
            "{}: wrapper died on SIGINT instead of surviving it",
            case.entry
        );
        std::thread::sleep(POLL);
    }

    // Step 4: exit the TUI its own way. claude clears input on the first
    // Ctrl-C and exits on the second; codex takes a second too. opencode exits
    // on the first, so the second lands on a dead pane.
    send_keys(&server, &session, "C-c");
    std::thread::sleep(Duration::from_millis(400));
    send_keys(&server, &session, "C-c");
    let exited = wait_until(EXIT_DEADLINE, || !pid_alive(wrapper));
    if !exited {
        let _ = tmux(&server, &["send-keys", "-t", &session, "-l", "/quit"]);
        send_keys(&server, &session, "Enter");
        assert!(
            wait_until(EXIT_DEADLINE, || !pid_alive(wrapper)),
            "{}: wrapper never exited after the TUI quit\n{}",
            case.entry,
            screen(&server, &session)
        );
    }
    assert_backend_stopped(&scratch, case);
    scratch.teardown(case.entry);
    Ok(())
}

/// The wrapper's exit path must leave no adapter backend behind.
fn assert_backend_stopped(scratch: &Scratch, case: &Case) {
    // The wrapper reaps its backend on the exit path; give it a beat.
    let gone = wait_until(EXIT_DEADLINE, || {
        scratch.backend_pids.iter().all(|pid| !pid_alive(*pid))
    });
    assert!(
        gone,
        "{}: backend survived the wrapper {:?}",
        case.entry, scratch.backend_pids
    );
}

/// Poll a predicate until true or the deadline passes.
fn wait_until<F: FnMut() -> bool>(deadline: Duration, mut probe: F) -> bool {
    let deadline = Instant::now() + deadline;
    loop {
        if probe() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(POLL);
    }
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

/// RECEIPT. Ctrl-C in an opencode `boop tui` pane leaves the wrapper alive and
/// its `opencode serve` backend is not orphaned when the TUI exits. Sabotage:
/// dropping the SIGINT registration kills the wrapper on the first Ctrl-C and
/// strands the serve process.
#[test]
fn ctrl_c_leaves_no_orphaned_backend_opencode() {
    let _case = CASE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    match run_one("opencode") {
        Ok(()) => println!("pass opencode"),
        Err(reason) => println!("skip opencode: {reason}"),
    }
}

/// RECEIPT, claude wrapper. Same body as the opencode case; see
/// `ctrl_c_leaves_no_orphaned_backend_opencode`.
#[test]
fn ctrl_c_leaves_no_orphaned_backend_claude() {
    let _case = CASE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    match run_one("claude") {
        Ok(()) => println!("pass claude"),
        Err(reason) => println!("skip claude: {reason}"),
    }
}

/// RECEIPT, codex wrapper. Same body as the opencode case; see
/// `ctrl_c_leaves_no_orphaned_backend_opencode`.
#[test]
fn ctrl_c_leaves_no_orphaned_backend_codex() {
    let _case = CASE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    match run_one("codex") {
        Ok(()) => println!("pass codex"),
        Err(reason) => println!("skip codex: {reason}"),
    }
}
