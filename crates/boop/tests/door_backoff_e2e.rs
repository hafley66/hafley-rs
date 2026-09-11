//! Door backoff end to end: a dead or stalled harness door never stalls a
//! boop verb. Every case runs per coordinator harness H in
//! {claude, codex, opencode}: one real binary, one real `boop tui H` on a
//! loopback llmock provider, one scratch store and a throwaway tmux server.
//!
//! The dead-door case kills H's door endpoint while its pane stays alive
//! (claude: removes the messaging socket; codex: removes the remote app-server
//! socket; opencode: kills the borrowed `opencode serve`). The slow-door case
//! SIGSTOPs that same endpoint. Recovery restarts a live coordinator on the
//! same route and watches the held hails land.
//!
//! Skips a harness only when its executable or `llmock` is absent, printed:
//!   cargo install --git https://github.com/larsakerlund/llmock.git \
//!     --tag v0.1.2 --locked llmock
//! Executable overrides: CLAUDE_BIN (ccz rides this), CODEX_BIN, OPENCODE_BIN,
//! LLMOCK_BIN.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use boop::harness::mock_tui::{self, MockProvider, MockTuiLaunch, MockTuiReplay};
use boop::harness::{shell_quote, HarnessId};
use boop::Registry;

const BOOP: &str = env!("CARGO_BIN_EXE_boop");

/// One real TUI, one llmock, one tmux server per case share heavy machine
/// resources; run the cases one at a time.
static CASE_LOCK: Mutex<()> = Mutex::new(());

/// A real TUI starts and a first turn runs here.
const STEP_DEADLINE: Duration = Duration::from_secs(90);
/// The poll interval under every deadline loop.
const POLL: Duration = Duration::from_millis(250);
/// A door call is bounded at five seconds; the slow case must return under
/// this, well before the pre-fix unbounded walk.
const SLOW_BOUND: Duration = Duration::from_secs(15);
/// A dead door must not stall a plain read.
const FAST_BOUND: Duration = Duration::from_secs(1);
/// The short cool-off the recovery case plants through the environment.
const RECOVER_COOLDOWN_SECS: u64 = 2;
/// A claude socket write larger than the 16 KiB kernel buffers, so a stopped
/// reader parks the write at its deadline rather than absorbing it.
const SLOW_BODY_BYTES: usize = 64 * 1024;

/// llmock answers the readiness probe with `boop` and every other turn with the
/// canned terminal reply the drivers wait on.
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
];

/// The three cases.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Dead,
    Slow,
    Recovery,
}

/// One scratch world: a throwaway tmux server, the coordinator pane, the
/// scratch home/mail, and the provider the pane talks to.
struct Scratch {
    server: String,
    session: String,
    route: String,
    root: PathBuf,
    home: PathBuf,
    mail: PathBuf,
    workspace: PathBuf,
    /// A scratch bin dir that shadows `open` for every harness process.
    bin: PathBuf,
    _provider: Option<MockProvider>,
    launch: Option<MockTuiLaunch>,
    /// opencode only: the borrowed `opencode serve` and its base URL.
    server_child: Option<Child>,
    server_base: Option<String>,
}

impl Scratch {
    fn db(&self) -> PathBuf {
        self.mail.join("boop.db")
    }

    /// The boop command against this scratch store. `BOOP_READER_HOME` points
    /// at the scratch home so the claude door reads this pane's registry.
    fn command(&self, args: &[&str], envs: &[(&str, &str)]) -> Command {
        use boop_store::testing::BoopCommandExt;
        let mut command = Command::new(BOOP);
        command
            .args(args)
            .boop_test_root(&self.root)
            .env("BOOP_DB", self.db())
            .env("BOOP_READER_HOME", &self.home);
        for (key, value) in envs {
            command.env(key, value);
        }
        command
    }

    fn boop_with(&self, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
        self.command(args, envs).output().expect("run boop")
    }

    fn boop(&self, args: &[&str]) -> std::process::Output {
        self.boop_with(args, &[])
    }

    /// The last non-empty line of a single-value `boop db` query.
    fn cell(&self, sql: &str) -> String {
        let out = self.boop(&["db", "--format", "text", sql]);
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .next_back()
            .unwrap_or("")
            .trim()
            .to_owned()
    }

    fn scalar(&self, sql: &str) -> i64 {
        self.cell(sql).parse::<i64>().unwrap_or(0)
    }

    fn route_session(&self) -> String {
        self.cell(&format!(
            "SELECT COALESCE(session_id,'') FROM agent_route WHERE route = '{}'",
            self.route
        ))
    }

    fn app_server_socket(&self) -> String {
        self.cell(&format!(
            "SELECT COALESCE(app_server_socket,'') FROM agent_route WHERE route = '{}'",
            self.route
        ))
    }

    /// Start the coordinator pane. opencode borrows an `opencode serve` the
    /// test owns, so killing that server leaves the wrapper (and the pane)
    /// alive with a dead door instead of triggering a wrapper respawn.
    fn start(&mut self, case: &Case) {
        let mut env = self.launch.as_ref().expect("launch").env.clone();
        env.extend(gui_shield(&self.bin));
        if case.id == HarnessId::Opencode {
            let base = self.start_opencode_server();
            env.push(("BOOP_OPENCODE_BASE".into(), base));
        }
        let launch = self.launch.as_ref().expect("launch");
        let command = pane_command(case, launch, &env, &self.db(), &self.workspace, &self.mail);
        let output = tmux(
            &self.server,
            &[
                "new-session",
                "-d",
                "-x",
                "200",
                "-y",
                "50",
                "-s",
                &self.session,
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

    /// Drive the recipe's first turn so the route binds a session.
    fn drive(&self, case: &Case) {
        let launch = self.launch.as_ref().expect("launch");
        if let MockTuiReplay::TypePrompt { readiness } = launch.replay {
            wait_for_screen(&self.server, case, &self.session, readiness, "readiness");
            let _ = tmux(
                &self.server,
                &[
                    "send-keys",
                    "-t",
                    &self.session,
                    "-l",
                    mock_tui::MOCK_PROMPT,
                ],
            );
            std::thread::sleep(Duration::from_millis(250));
            let _ = tmux(&self.server, &["send-keys", "-t", &self.session, "Enter"]);
        }
        wait_for_screen(
            &self.server,
            case,
            &self.session,
            mock_tui::MOCK_REPLY_MARKER,
            "reply",
        );
        wait_for_bound(self, case);
    }

    /// Tear the pane down and start a fresh coordinator on the same route. The
    /// wrapper holds the route lock and a live session pid until it exits, so
    /// wait for the old pane to be gone before starting the replacement.
    fn restart(&mut self, case: &Case) {
        let old_pane = self.pane_pid();
        let _ = tmux(&self.server, &["kill-session", "-t", &self.session]);
        let deadline = Instant::now() + STEP_DEADLINE;
        while pid_alive(old_pane) {
            assert!(
                Instant::now() < deadline,
                "{}: old wrapper {old_pane} never exited",
                case.entry
            );
            std::thread::sleep(POLL);
        }
        self.stop_server();
        self.kill_survivors();
        // The route lock and the live session pid are released on process
        // exit; give the store projection a beat to observe the death too.
        std::thread::sleep(Duration::from_millis(500));
        self.start(case);
        self.drive(case);
    }

    /// Start a borrowed `opencode serve` with the recipe env and wait for it.
    fn start_opencode_server(&mut self) -> String {
        let launch = self.launch.as_ref().expect("launch");
        let port = free_port();
        let base = format!("http://127.0.0.1:{port}/");
        let mut command = Command::new(&launch.executable);
        command
            .args([
                "serve",
                "--port",
                &port.to_string(),
                "--hostname",
                "127.0.0.1",
            ])
            .envs(launch.env.iter().cloned())
            .envs(gui_shield(&self.bin))
            .env("OPENCODE_DISABLE_AUTOUPDATE", "1")
            .current_dir(&self.workspace)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // Its own process group, so the pane closing cannot HUP it and the
        // teardown can reach the server and any worker it spawned.
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let child = command.spawn().expect("start opencode serve");
        let deadline = Instant::now() + STEP_DEADLINE;
        while !http_ok(port) {
            assert!(
                Instant::now() < deadline,
                "opencode serve never answered on :{port}"
            );
            std::thread::sleep(POLL);
        }
        self.server_child = Some(child);
        self.server_base = Some(base.clone());
        base
    }

    fn stop_server(&mut self) {
        if let Some(mut child) = self.server_child.take() {
            let pid = child.id();
            let _ = Command::new("kill")
                .args(["-KILL", &format!("-{pid}")])
                .output();
            let _ = Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .output();
            let _ = child.wait();
        }
    }

    /// The door endpoint this case breaks or stops.
    fn endpoint(&self, case: &Case) -> Endpoint {
        match case.id {
            HarnessId::Claude => claude_endpoint(self),
            HarnessId::Codex => codex_endpoint(self),
            HarnessId::Opencode => Endpoint::Opencode {
                pid: self
                    .server_child
                    .as_ref()
                    .expect("borrowed opencode server")
                    .id(),
                base: self.server_base.clone().expect("opencode base"),
            },
            HarnessId::Kimi => unreachable!("kimi has no door endpoint in this matrix"),
        }
    }

    /// The pid the pane leads with, so a test can prove the pane survived.
    fn pane_pid(&self) -> u32 {
        pane_pid(&self.server, &self.session)
    }

    /// Kill and clean up anything this case left behind.
    fn kill_survivors(&self) {
        if let Some(child) = &self.server_child {
            let pid = child.id();
            let _ = Command::new("kill")
                .args(["-KILL", &format!("-{pid}")])
                .output();
        }
        for pid in self.scratch_processes() {
            if !pid_alive(pid) {
                continue;
            }
            let _ = Command::new("kill")
                .args(["-KILL", &format!("-{pid}")])
                .output();
            let _ = Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .output();
        }
    }

    /// Every process whose cwd or `HOME` sits under this case's scratch root,
    /// found even when the case panicked before recording its endpoints.
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
        let _ = tmux(&self.server, &["kill-server"]);
        self.stop_server();
        self.kill_survivors();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The door endpoint whose death or stall the test inflicts.
enum Endpoint {
    /// claude: the messaging socket and the TUI process that owns it.
    Claude { socket: PathBuf, pid: u32 },
    /// codex: the remote app-server socket and its process.
    Codex { socket: PathBuf, pid: u32 },
    /// opencode: the borrowed HTTP server.
    Opencode { pid: u32, base: String },
}

impl Endpoint {
    fn pid(&self) -> u32 {
        match self {
            Endpoint::Claude { pid, .. } => *pid,
            Endpoint::Codex { pid, .. } => *pid,
            Endpoint::Opencode { pid, .. } => *pid,
        }
    }

    /// Kill the endpoint: unlink a socket, or kill the HTTP server.
    fn kill(&self) {
        match self {
            Endpoint::Claude { socket, .. } | Endpoint::Codex { socket, .. } => {
                std::fs::remove_file(socket).expect("remove door socket");
            }
            Endpoint::Opencode { pid, .. } => {
                let _ = Command::new("kill")
                    .args(["-KILL", &pid.to_string()])
                    .output();
            }
        }
    }

    fn stop(&self) {
        let _ = Command::new("kill")
            .args(["-STOP", &self.pid().to_string()])
            .output();
    }

    fn resume(&self) {
        let _ = Command::new("kill")
            .args(["-CONT", &self.pid().to_string()])
            .output();
    }

    fn describe(&self) -> String {
        match self {
            Endpoint::Claude { socket, .. } => format!("claude socket {}", socket.display()),
            Endpoint::Codex { socket, .. } => format!("codex socket {}", socket.display()),
            Endpoint::Opencode { base, .. } => format!("opencode {base}"),
        }
    }
}

/// Create the scratch bin dir and shadow `open` with a no-op, so a harness
/// that decides to raise a browser cannot steal the operator's desktop focus.
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

fn tmux(server: &str, args: &[&str]) -> std::process::Output {
    let mut full = vec!["-L", server];
    full.extend_from_slice(args);
    Command::new("tmux").args(&full).output().expect("run tmux")
}

fn screen(server: &str, session: &str) -> String {
    let output = tmux(server, &["capture-pane", "-p", "-t", session, "-S", "-200"]);
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn wait_for_screen(server: &str, case: &Case, session: &str, wanted: &str, label: &str) {
    let deadline = Instant::now() + STEP_DEADLINE;
    loop {
        let text = screen(server, session);
        if text.contains(wanted) {
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

fn wait_for_bound(scratch: &Scratch, case: &Case) {
    let deadline = Instant::now() + STEP_DEADLINE;
    loop {
        let session = scratch.route_session();
        if !session.trim().is_empty() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{}: route {} never bound a session",
            case.entry,
            scratch.route
        );
        std::thread::sleep(POLL);
    }
}

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

/// The command tmux runs in the pane: the recipe env, the scratch store, then
/// the wrapped real TUI. Extra env carries the borrowed opencode base.
fn pane_command(
    case: &Case,
    launch: &MockTuiLaunch,
    env: &[(String, String)],
    db: &Path,
    workspace: &Path,
    mail: &Path,
) -> String {
    let mut command = String::from("exec env");
    for (key, value) in env {
        command.push_str(&format!(" {}={}", key, shell_quote(value)));
    }
    command.push_str(&format!(
        " BOOP_DB={} BOOP_NO_SYNC=1",
        shell_quote(&db.display().to_string())
    ));
    command.push_str(&format!(
        " {} tui {} --name {} --bin {} --cwd {} --mail-dir {} --",
        shell_quote(BOOP),
        case.entry,
        shell_quote(&format!("door-backoff-{}", case.entry)),
        shell_quote(&launch.executable),
        shell_quote(&workspace.display().to_string()),
        shell_quote(&mail.display().to_string()),
    ));
    for arg in &launch.args {
        command.push(' ');
        command.push_str(&shell_quote(arg));
    }
    command
}

/// claude's messaging socket and owner pid, read from its session registry.
fn claude_endpoint(scratch: &Scratch) -> Endpoint {
    let session = scratch.route_session();
    let dir = scratch.home.join(".claude").join("sessions");
    let deadline = Instant::now() + STEP_DEADLINE;
    loop {
        if let Some((socket, pid)) = claude_registry(&dir, &session) {
            return Endpoint::Claude { socket, pid };
        }
        assert!(
            Instant::now() < deadline,
            "claude endpoint for {session} never appeared under {}",
            dir.display()
        );
        std::thread::sleep(POLL);
    }
}

fn claude_registry(dir: &Path, session: &str) -> Option<(PathBuf, u32)> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if value.get("sessionId").and_then(|value| value.as_str()) != Some(session) {
            continue;
        }
        let socket = value
            .get("messagingSocketPath")
            .and_then(|value| value.as_str())?;
        let pid = value.get("pid").and_then(|value| value.as_u64())? as u32;
        if socket.is_empty() {
            continue;
        }
        return Some((PathBuf::from(socket), pid));
    }
    None
}

/// codex's remote app-server socket and the process listening on it.
fn codex_endpoint(scratch: &Scratch) -> Endpoint {
    let socket = PathBuf::from(scratch.app_server_socket());
    assert!(
        !socket.as_os_str().is_empty(),
        "codex route names no socket"
    );
    let deadline = Instant::now() + STEP_DEADLINE;
    loop {
        if socket.exists() {
            if let Some(pid) = app_server_pid(&socket) {
                return Endpoint::Codex { socket, pid };
            }
        }
        assert!(
            Instant::now() < deadline,
            "codex app-server at {} never appeared",
            socket.display()
        );
        std::thread::sleep(POLL);
    }
}

/// The pid whose command line runs the app-server on `socket`.
fn app_server_pid(socket: &Path) -> Option<u32> {
    let needle = socket.to_string_lossy();
    let output = Command::new("ps")
        .args(["-E", "-ww", "-o", "pid=,command="])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| {
            let (pid, rest) = line.trim().split_once(char::is_whitespace)?;
            (rest.contains(needle.as_ref()) && rest.contains("app-server"))
                .then(|| pid.parse().ok())
                .flatten()
        })
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn http_ok(port: u16) -> bool {
    let Ok(mut stream) = std::net::TcpStream::connect(("127.0.0.1", port)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let request = format!("GET /session HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut buffer = [0u8; 64];
    let read = stream.read(&mut buffer).unwrap_or(0);
    let head = String::from_utf8_lossy(&buffer[..read]);
    head.starts_with("HTTP/1.") && head.contains(" 200")
}

/// Combined stdout and stderr of one process.
fn combined(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn assert_names_door_and_route(output: &std::process::Output, door: &str, route: &str) {
    let text = combined(output);
    assert!(text.contains(door), "no door name `{door}` in:\n{text}");
    assert!(text.contains(route), "no route name `{route}` in:\n{text}");
}

/// A beep run with its own deadline: a door that ignores its client timeout
/// must fail the test at `SLOW_BOUND`, never hang the runner.
fn run_bounded(
    scratch: &Scratch,
    args: &[&str],
    envs: &[(&str, &str)],
) -> (bool, Duration, String) {
    let out_path = scratch.root.join("bounded.out");
    let err_path = scratch.root.join("bounded.err");
    let mut child = scratch
        .command(args, envs)
        .stdout(Stdio::from(std::fs::File::create(&out_path).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(&err_path).unwrap()))
        .spawn()
        .expect("spawn bounded boop");
    let started = Instant::now();
    let finished = loop {
        if child.try_wait().expect("poll bounded boop").is_some() {
            break true;
        }
        if started.elapsed() >= SLOW_BOUND {
            let _ = child.kill();
            let _ = child.wait();
            break false;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let elapsed = started.elapsed();
    let text = format!(
        "{}{}",
        std::fs::read_to_string(&out_path).unwrap_or_default(),
        std::fs::read_to_string(&err_path).unwrap_or_default()
    );
    (finished, elapsed, text)
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

// ---------------------------------------------------------------------------
// The cases.
// ---------------------------------------------------------------------------

fn case_dead(scratch: &Scratch, case: &Case) {
    let endpoint = scratch.endpoint(case);
    endpoint.kill();
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        pid_alive(scratch.pane_pid()),
        "{}: pane died when its door endpoint died",
        case.entry
    );

    let mut first = None;
    for index in 0..20 {
        let output = scratch.boop(&["beep", &scratch.route, &format!("hi-{index}"), "--no-wait"]);
        if first.is_none() {
            first = Some(output);
        }
    }
    assert_names_door_and_route(&first.unwrap(), case.entry, &scratch.route);

    // The first failed push records one cool-off; every later push skips the
    // route, so the dead transport is walked once, not once per held row.
    let blowouts = scratch.scalar(&format!(
        "SELECT COUNT(*) FROM agent_door_blowout \
         WHERE route = '{}' AND why LIKE 'door-unreachable%'",
        scratch.route
    ));
    assert_eq!(
        blowouts,
        1,
        "{}: expected one door-unreachable cool-off ({})",
        case.entry,
        endpoint.describe()
    );
    let attempts = scratch.scalar(&format!(
        "SELECT COUNT(*) FROM agent_delivery_transition \
         WHERE route = '{}' AND detail LIKE '% door for %'",
        scratch.route
    ));
    assert!(
        attempts <= 1,
        "{}: the dead door was walked {attempts} times",
        case.entry
    );

    // A plain read still triggers the startup drain; the cooled route is
    // skipped, so it stays fast where the pre-fix walk paid the dead door.
    let _ = scratch.boop(&["db", "SELECT 1"]);
    let started = Instant::now();
    let timed = scratch.boop(&["db", "SELECT 1"]);
    let elapsed = started.elapsed();
    assert!(timed.status.success(), "{}: boop db failed", case.entry);
    assert!(
        elapsed < FAST_BOUND,
        "{}: boop db took {elapsed:?} with a dead door",
        case.entry
    );
    println!("pass {}: dead door cooled after {elapsed:?}", case.entry);
}

fn case_slow(scratch: &Scratch, case: &Case) {
    let endpoint = scratch.endpoint(case);
    endpoint.stop();
    let body = if case.id == HarnessId::Claude {
        "x".repeat(SLOW_BODY_BYTES)
    } else {
        "hi".to_owned()
    };
    let (finished, elapsed, text) =
        run_bounded(scratch, &["beep", &scratch.route, &body, "--no-wait"], &[]);
    endpoint.resume();
    assert!(
        finished,
        "{}: a stopped door stalled the send past {SLOW_BOUND:?} ({})",
        case.entry,
        endpoint.describe()
    );
    assert!(
        elapsed < SLOW_BOUND,
        "{}: a stopped door stalled the send for {elapsed:?}",
        case.entry
    );
    assert!(
        text.contains(case.entry),
        "{}: the slow door call was not named:\n{text}",
        case.entry
    );
    assert!(
        text.contains(&scratch.route),
        "{}: the slow route was not named:\n{text}",
        case.entry
    );
    println!("pass {}: slow door bounded at {elapsed:?}", case.entry);
}

fn case_recovery(scratch: &mut Scratch, case: &Case) {
    let endpoint = scratch.endpoint(case);
    endpoint.kill();
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        pid_alive(scratch.pane_pid()),
        "{}: pane died when its door endpoint died",
        case.entry
    );

    // Queue hails while the route is dead; the first records a two-second
    // cool-off, so the rest are held without re-walking the dead transport.
    for index in 0..3 {
        let _ = scratch.boop_with(
            &[
                "beep",
                &scratch.route,
                &format!("recover-{index}"),
                "--no-wait",
            ],
            &[(
                "BOOP_DOOR_FAIL_COOLDOWN_SECS",
                &RECOVER_COOLDOWN_SECS.to_string(),
            )],
        );
    }
    let blowouts = scratch.scalar(&format!(
        "SELECT COUNT(*) FROM agent_door_blowout \
         WHERE route = '{}' AND why LIKE 'door-unreachable%'",
        scratch.route
    ));
    assert!(blowouts >= 1, "{}: the route never cooled off", case.entry);

    // Restart a live coordinator on the same route. The new pane binds a new
    // session; the held hails are still addressed to the route.
    scratch.restart(case);
    std::thread::sleep(Duration::from_millis(RECOVER_COOLDOWN_SECS * 1000 + 500));

    let deadline = Instant::now() + STEP_DEADLINE;
    loop {
        let landed = scratch.scalar(&format!(
            "SELECT COUNT(*) FROM agent_delivery_transition \
             WHERE route = '{}' \
             AND outcome IN ('accepted-by-harness','held-for-turn-boundary')",
            scratch.route
        ));
        if landed >= 1 {
            println!("pass {}: recovered {landed} held hail(s)", case.entry);
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{}: the held hails never reached the restarted route\nroute: {}\ntransitions:\n{}\nblowouts:\n{}",
            case.entry,
            scratch.route_session(),
            scratch.cell(&format!(
                "SELECT outcome || ' :: ' || detail FROM agent_delivery_transition \
                 WHERE route = '{}' ORDER BY sequence",
                scratch.route
            )),
            scratch.cell(&format!(
                "SELECT why || ' :: ' || at_ms || ' :: ' || cooldown_ms \
                 FROM agent_door_blowout WHERE route = '{}'",
                scratch.route
            )),
        );
        let _ = scratch.boop(&["db", "SELECT 1"]);
        std::thread::sleep(POLL);
    }
}

/// One harness end to end for one case. `Err` is a skip reason.
fn run_case(case: &Case, kind: Kind, llmock: &Path, registry: &Registry) -> Result<(), String> {
    let root = std::env::temp_dir().join(format!(
        "boop-doorbackoff-{}-{}",
        case.entry,
        std::process::id()
    ));
    let server = format!("boop-doorbackoff-{}-{}", case.entry, std::process::id());
    let session = format!("boop-doorbackoff-{}-{}", case.entry, std::process::id());
    let _ = std::fs::remove_dir_all(&root);
    let _ = tmux(&server, &["kill-server"]);
    for dir in ["mail", "home", "workspace"] {
        std::fs::create_dir_all(root.join(dir)).map_err(|error| format!("mkdir {dir}: {error}"))?;
    }
    let bin = root.join("bin");
    write_gui_shield(&bin);
    let fixture = root.join("llmock.yaml");
    std::fs::write(&fixture, FIXTURE_YAML).map_err(|error| format!("write fixture: {error}"))?;
    let provider = mock_tui::MockProvider::spawn(llmock, Some(&fixture))
        .map_err(|error| format!("llmock spawn: {error}"))?;
    let launch = registry
        .get(case.id)
        .mock_tui_launch(&mock_tui::MockTuiContext {
            home: &root.join("home"),
            workspace: &root.join("workspace"),
            port: provider.port,
        })
        .map_err(|error| format!("{error}"))?;

    let mut scratch = Scratch {
        server,
        session,
        route: format!("door-backoff-{}", case.entry),
        root: root.clone(),
        home: root.join("home"),
        mail: root.join("mail"),
        workspace: root.join("workspace"),
        bin,
        _provider: Some(provider),
        launch: Some(launch),
        server_child: None,
        server_base: None,
    };
    scratch.start(case);
    scratch.drive(case);

    match kind {
        Kind::Dead => case_dead(&scratch, case),
        Kind::Slow => case_slow(&scratch, case),
        Kind::Recovery => case_recovery(&mut scratch, case),
    }
    Ok(())
}

/// Resolve the shared prerequisites, then run one harness for one case. Absent
/// executable or llmock is a skip reason, not a failure.
fn run_one(entry: &str, kind: Kind) -> Result<(), String> {
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
    run_case(case, kind, &llmock, &registry)
}

/// One test body: hold the case lock, skip on a missing prerequisite.
fn run(entry: &str, kind: Kind) {
    let _case = CASE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    match run_one(entry, kind) {
        Ok(()) => {}
        Err(reason) => eprintln!("skip {entry} {kind:?}: {reason}"),
    }
}

/// RECEIPT. A dead claude messaging socket leaves the pane alive, records one
/// cool-off, names the door on the first send, and keeps `boop db` fast.
#[test]
fn dead_door_claude() {
    run("claude", Kind::Dead);
}

/// RECEIPT, codex. Same body; see `dead_door_claude`.
#[test]
fn dead_door_codex() {
    run("codex", Kind::Dead);
}

/// RECEIPT, opencode. Same body; see `dead_door_claude`.
#[test]
fn dead_door_opencode() {
    run("opencode", Kind::Dead);
}

/// RECEIPT. A SIGSTOPped claude messaging socket parks the blocked write at
/// its deadline; the send returns well inside the bound and names the door.
#[test]
fn slow_door_claude() {
    run("claude", Kind::Slow);
}

/// RECEIPT, codex. Same body; see `slow_door_claude`.
#[test]
fn slow_door_codex() {
    run("codex", Kind::Slow);
}

/// RECEIPT, opencode. Same body; see `slow_door_claude`.
#[test]
fn slow_door_opencode() {
    run("opencode", Kind::Slow);
}

/// RECEIPT. After the short cool-off, a fresh claude coordinator on the same
/// route takes the hails the dead door left in the mailbox.
#[test]
fn recovery_claude() {
    run("claude", Kind::Recovery);
}

/// RECEIPT, codex. Same body; see `recovery_claude`.
#[test]
fn recovery_codex() {
    run("codex", Kind::Recovery);
}

/// RECEIPT, opencode. Same body; see `recovery_claude`.
#[test]
fn recovery_opencode() {
    run("opencode", Kind::Recovery);
}
