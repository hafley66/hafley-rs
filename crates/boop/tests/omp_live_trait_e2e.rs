//! OMP's real terminal records bind a live tmux pane to the transcript UUID
//! that Instant attribution reads. This receipt runs two real OMP TUIs through
//! `boop tui omp`, against llmock, on a test-owned tmux socket and git repo.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use boop::harness::mock_tui::{self, MockTuiContext, MockTuiLaunch, MockTuiReplay};
use boop::harness::{shell_quote, HarnessId};
use boop::live::session_in_pane_on_socket;
use boop::Registry;
use boop_store::ident::{Store, TurnQuery};
use boop_store::testing::BoopCommandExt;

const BOOP: &str = env!("CARGO_BIN_EXE_boop");
const POLL: Duration = Duration::from_millis(100);
const DEADLINE: Duration = Duration::from_secs(75);
static LIVE_ENV: OnceLock<Mutex<()>> = OnceLock::new();
static NEXT_SCRATCH: AtomicUsize = AtomicUsize::new(0);

struct Scratch {
    root: PathBuf,
    socket: String,
}

impl Scratch {
    fn new() -> Self {
        let unique = format!(
            "{}-{}-{}",
            std::process::id(),
            boop::live::now_ms(),
            NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed)
        );
        let root = std::env::temp_dir().join(format!("boop-omp-live-trait-{unique}"));
        let socket = format!("boop-omp-live-trait-{unique}");
        let _ = std::fs::remove_dir_all(&root);
        let _ = tmux(&socket, &["kill-server"]);
        for dir in ["home", "mail", "repo", "tmp"] {
            std::fs::create_dir_all(root.join(dir)).expect("create scratch directory");
        }
        git(&root.join("repo"), &["init", "-q", "-b", "main"]);
        git(
            &root.join("repo"),
            &["config", "user.email", "boop@example.invalid"],
        );
        git(&root.join("repo"), &["config", "user.name", "Boop OMP E2E"]);
        std::fs::write(root.join("repo/seed.txt"), "seed\n").expect("write seed");
        git(&root.join("repo"), &["add", "seed.txt"]);
        git(&root.join("repo"), &["commit", "-qm", "seed"]);
        Self { root, socket }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn agent_dir(&self) -> PathBuf {
        self.home().join(".omp/agent")
    }

    fn mail(&self) -> PathBuf {
        self.root.join("mail")
    }

    fn repo(&self) -> PathBuf {
        self.root.join("repo")
    }

    fn db(&self) -> PathBuf {
        self.mail().join("boop.db")
    }

    fn tmp(&self) -> PathBuf {
        self.root.join("tmp")
    }

    fn pane(&self, session: &str) -> String {
        let output = tmux(
            &self.socket,
            &["display-message", "-p", "-t", session, "#{pane_id}"],
        );
        assert!(output.status.success(), "read pane id: {output:?}");
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn tmux_context(&self) -> String {
        let output = tmux(
            &self.socket,
            &["display-message", "-p", "#{socket_path},#{pid},0"],
        );
        assert!(
            output.status.success(),
            "read scratch TMUX context: {output:?}"
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn screen(&self, session: &str) -> String {
        let output = tmux(
            &self.socket,
            &["capture-pane", "-p", "-t", session, "-S", "-300"],
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = tmux(&self.socket, &["kill-server"]);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct EnvGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: the live test holds LIVE_ENV while it owns these variables.
        unsafe { std::env::set_var(key, value) };
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        // SAFETY: the live test still holds LIVE_ENV while its guards drop.
        unsafe {
            match &self.previous {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
}

fn tmux(socket: &str, args: &[&str]) -> std::process::Output {
    let mut full = vec!["-L", socket];
    full.extend_from_slice(args);
    Command::new("tmux").args(full).output().expect("run tmux")
}

fn git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn wait(label: &str, predicate: impl Fn() -> bool, screen: impl Fn() -> String) {
    let deadline = Instant::now() + DEADLINE;
    while !predicate() {
        assert!(Instant::now() < deadline, "{label} timed out\n{}", screen());
        std::thread::sleep(POLL);
    }
}

fn tmux_env(command: &Command) -> String {
    let mut shell = String::from("exec env -i");
    let envs: Vec<_> = command.get_envs().collect();
    for key in envs
        .iter()
        .filter_map(|(key, value)| value.is_none().then_some(*key))
    {
        let key = shell_quote(&key.to_string_lossy());
        shell.push_str(&format!(" -u {key}"));
    }
    for (key, value) in envs
        .iter()
        .filter_map(|(key, value)| value.as_ref().map(|value| (*key, *value)))
    {
        let key = shell_quote(&key.to_string_lossy());
        shell.push_str(&format!(" {key}={}", shell_quote(&value.to_string_lossy())));
    }
    // `boop_test_root` removes inherited route identity. The pane's own
    // socket identity is supplied by tmux when this command is evaluated.
    shell.push_str(" TMUX=\"$TMUX\" TMUX_PANE=\"$TMUX_PANE\"");
    shell
}

fn tmux_command(command: &Command) -> String {
    let mut shell = tmux_env(command);
    shell.push(' ');
    shell.push_str(&shell_quote(&command.get_program().to_string_lossy()));
    for arg in command.get_args() {
        shell.push(' ');
        shell.push_str(&shell_quote(&arg.to_string_lossy()));
    }
    shell
}

#[test]
fn tmux_command_preserves_homes_and_serializes_fixture_overrides() {
    let _env_lock = LIVE_ENV.get_or_init(|| Mutex::new(())).lock().unwrap();
    let scratch = Scratch::new();
    let launch = MockTuiLaunch {
        executable: "/tmp/omp-fixture".into(),
        args: vec!["--model".into(), "llmock/mock-model".into()],
        env: vec![
            ("HOME".into(), scratch.home().display().to_string()),
            ("TMPDIR".into(), "/ambient/tmp".into()),
            (
                "PI_CODING_AGENT_DIR".into(),
                scratch.agent_dir().display().to_string(),
            ),
        ],
        config_paths: Vec::new(),
        replay: MockTuiReplay::PromptArg,
    };
    let command = omp_command(
        &scratch,
        &launch,
        Path::new("/tmp/boop-fixture"),
        "omp-command-test",
        &[],
    );
    let shell = tmux_env(&command);
    let output = Command::new("sh")
        .env("BOOP_ROUTE_TEST", "outer-stale")
        .args(["-c", &format!("{shell} env")])
        .output()
        .expect("run serialized command environment");
    assert!(
        output.status.success(),
        "serialized command failed: {output:?}"
    );
    let env = String::from_utf8_lossy(&output.stdout);
    let has = |name: &str, value: &Path| {
        env.lines()
            .any(|line| line == format!("{name}={}", value.display()))
    };
    assert!(has("BOOP_READER_HOME", &scratch.home()));
    assert!(has(
        "BOOP_CONFIG",
        &scratch.home().join("config/boop/config.json")
    ));
    assert!(has("BOOP_DB", &scratch.db()));
    assert!(has("BOOP_MAIL_DIR", &scratch.mail()));
    assert!(has("PI_CODING_AGENT_DIR", &scratch.agent_dir()));
    assert!(has("TMPDIR", &scratch.tmp()));
    assert!(!env.lines().any(|line| line == "TMPDIR=/ambient/tmp"));
    assert!(!env.lines().any(|line| line.starts_with("BOOP_ROUTE_TEST=")));
    if let Some(home) = std::env::var_os("HOME") {
        assert!(env
            .lines()
            .any(|line| line == format!("HOME={}", home.to_string_lossy())));
    }
    assert!(!env
        .lines()
        .any(|line| line == format!("HOME={}", scratch.home().display())));
    if let Some(codex_home) = std::env::var_os("CODEX_HOME") {
        assert!(env
            .lines()
            .any(|line| line == format!("CODEX_HOME={}", codex_home.to_string_lossy())));
    }
}

fn omp_command(
    scratch: &Scratch,
    launch: &MockTuiLaunch,
    executable: &Path,
    route: &str,
    extra_args: &[String],
) -> Command {
    let mut child = Command::new(BOOP);
    child.env_clear();
    for (key, value) in &launch.env {
        // terminal_env supplies HOME for ordinary adapter runs. This fixture
        // pins OMP's actual config through PI_CODING_AGENT_DIR and preserves
        // the real process homes below.
        if key != "HOME" {
            child.env(key, value);
        }
    }
    // The OMP recipe's PI_CODING_AGENT_DIR is the reader/config root. Keep
    // HOME and CODEX_HOME from the test process so the wrapper does not alter
    // either process-global home while the child still has fixture readers.
    for (key, value) in [
        ("HOME", std::env::var_os("HOME")),
        ("CODEX_HOME", std::env::var_os("CODEX_HOME")),
    ] {
        if let Some(value) = value {
            child.env(key, value);
        }
    }
    for (key, value) in [
        ("XDG_CONFIG_HOME", scratch.home().join(".config")),
        ("XDG_DATA_HOME", scratch.home().join(".local/share")),
        ("XDG_CACHE_HOME", scratch.home().join(".cache")),
        ("XDG_STATE_HOME", scratch.home().join(".local/state")),
        ("TMPDIR", scratch.tmp()),
    ] {
        child.env(key, value);
    }
    child
        .boop_test_root(&scratch.home())
        .env("BOOP_DB", scratch.db())
        .env("BOOP_MAIL_DIR", scratch.mail())
        .env("BOOP_NO_SYNC", "1")
        .env("BOOP_NATIVE_PROJECT_EVERY_MS", "100")
        .env("BOOP_NATIVE_DISCOVER_EVERY_MS", "100")
        .env("OMP_SKIP_SETUP", "1")
        .arg("tui")
        .arg("omp")
        .args(["--name", route, "--bin"])
        .arg(executable)
        .args(["--cwd"])
        .arg(scratch.repo())
        .args(["--mail-dir"])
        .arg(scratch.mail())
        .arg("--");
    child.args(&launch.args).args(extra_args).arg("--auto-approve");
    child
}

fn launch_omp(
    scratch: &Scratch,
    registry: &Registry,
    executable: &Path,
    port: u16,
    route: &str,
    session: &str,
    extra_args: &[String],
) {
    let adapter = registry.get(HarnessId::Omp);
    let launch = adapter
        .mock_tui_launch(&MockTuiContext {
            home: &scratch.home(),
            workspace: &scratch.repo(),
            port,
        })
        .expect("OMP mock-TUI launch recipe");
    let child = omp_command(scratch, &launch, executable, route, extra_args);
    let command = tmux_command(&child);
    let output = tmux(
        &scratch.socket,
        &[
            "new-session",
            "-d",
            "-x",
            "160",
            "-y",
            "48",
            "-s",
            session,
            &command,
        ],
    );
    assert!(
        output.status.success(),
        "start {session}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    if let MockTuiReplay::TypePrompt { readiness } = launch.replay {
        wait(
            &format!("{session} readiness {readiness:?}"),
            || scratch.screen(session).contains(readiness),
            || scratch.screen(session),
        );
        let _ = tmux(
            &scratch.socket,
            &["send-keys", "-t", session, "-l", mock_tui::MOCK_PROMPT],
        );
        let _ = tmux(&scratch.socket, &["send-keys", "-t", session, "Enter"]);
    }
    wait(
        &format!("{session} deterministic reply"),
        || {
            scratch
                .screen(session)
                .contains(mock_tui::MOCK_REPLY_MARKER)
        },
        || scratch.screen(session),
    );
}

fn route_session(scratch: &Scratch, route: &str) -> String {
    let deadline = Instant::now() + DEADLINE;
    loop {
        let found = boop_store::bus::read_routes(&scratch.mail())
            .expect("read product route")
            .remove(route)
            .and_then(|route| route.session_id)
            .filter(|id| !id.is_empty());
        if let Some(id) = found {
            return id;
        }
        assert!(
            Instant::now() < deadline,
            "route {route} never received OMP's session UUID"
        );
        std::thread::sleep(POLL);
    }
}

fn wait_for_turns(scratch: &Scratch, session: &str) {
    let deadline = Instant::now() + DEADLINE;
    loop {
        let store = Store::open(scratch.db()).expect("open product store");
        let rows = store
            .turn_rows(&TurnQuery {
                session: Some(session.to_owned()),
                ..Default::default()
            })
            .expect("read projected OMP turns");
        if rows
            .iter()
            .any(|row| row.role == "user" && row.said.contains(mock_tui::MOCK_PROMPT))
            && rows.iter().any(|row| {
                row.role == "assistant" && row.said.contains(mock_tui::MOCK_REPLY_MARKER)
            })
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "stored OMP turns missing for {session}: {rows:?}"
        );
        std::thread::sleep(POLL);
    }
}

/// RECEIPT. Two real OMP TUIs in the same scratch git cwd each create a native
/// transcript and terminal record. Registry dispatch, socket-aware pane lookup,
/// transcript messages, and Boop's stored turns preserve their distinct UUIDs.
#[test]
fn omp_live_panes_bind_distinct_sessions_and_project_real_transcripts() {
    let _env_lock = LIVE_ENV.get_or_init(|| Mutex::new(())).lock().unwrap();
    let Some(llmock) = mock_tui::resolve_llmock() else {
        eprintln!("skip omp_live_trait_e2e: no llmock (set LLMOCK_BIN)");
        return;
    };
    let Some(executable) = mock_tui::resolve_executable("omp", "OMP_BIN") else {
        eprintln!("skip omp_live_trait_e2e: no omp executable (set OMP_BIN)");
        return;
    };
    let scratch = Scratch::new();
    let _mail = EnvGuard::set("BOOP_MAIL_DIR", &scratch.mail());
    let _db = EnvGuard::set("BOOP_DB", &scratch.db());
    let _agent_dir = EnvGuard::set("PI_CODING_AGENT_DIR", &scratch.agent_dir());
    let registry = Registry::discover();
    let provider = mock_tui::MockProvider::spawn(&llmock, None).expect("start llmock");

    launch_omp(
        &scratch,
        &registry,
        &executable,
        provider.port,
        "omp-live-a",
        "omp-live-a",
        &[],
    );
    launch_omp(
        &scratch,
        &registry,
        &executable,
        provider.port,
        "omp-live-b",
        "omp-live-b",
        &[],
    );

    let pane_a = scratch.pane("omp-live-a");
    let pane_b = scratch.pane("omp-live-b");
    let _tmux = EnvGuard::set("TMUX", scratch.tmux_context());
    assert_ne!(pane_a, pane_b, "two real OMP TUIs share a pane");
    let session_a = route_session(&scratch, "omp-live-a");
    let session_b = route_session(&scratch, "omp-live-b");
    assert_ne!(
        session_a, session_b,
        "two real OMP TUIs reused a session UUID"
    );

    let adapter = registry.get(HarnessId::Omp);
    for (pane, expected) in [(&pane_a, &session_a), (&pane_b, &session_b)] {
        let live = adapter
            .live()
            .live_session_in_pane_on_socket(pane, Some(&scratch.socket))
            .expect("query OMP live binding")
            .expect("real OMP pane binding");
        assert_eq!(
            &live.session_id, expected,
            "OMP terminal record bound wrong pane"
        );
        let dispatched =
            session_in_pane_on_socket(&registry, pane, Some(&scratch.socket), &scratch.mail())
                .expect("Registry socket-aware dispatch");
        assert_eq!(dispatched.as_deref(), Some(expected.as_str()));

        let session = adapter
            .session_by_id(expected, Some(&scratch.repo().display().to_string()))
            .expect("real OMP transcript by UUID");
        let chunk = adapter
            .read_from(&session, 0)
            .expect("read real OMP transcript");
        assert!(
            !chunk.events.is_empty(),
            "OMP transcript has no adapter events"
        );
        let messages = adapter.messages(&session, None);
        assert!(
            messages.iter().any(|message| message.role == "assistant"
                && message.text.contains(mock_tui::MOCK_REPLY_MARKER)),
            "OMP transcript trait omitted replayed reply"
        );
        wait_for_turns(&scratch, expected);
    }
    assert_eq!(
        boop::live::session_in_pane(&registry, &pane_a, &scratch.mail())
            .expect("Registry inherited-TMUX dispatch")
            .as_deref(),
        Some(session_a.as_str())
    );

    let _ = tmux(&scratch.socket, &["kill-session", "-t", "omp-live-a"]);
    wait(
        "dead OMP pane binding",
        || {
            adapter
                .live()
                .live_session_in_pane_on_socket(&pane_a, Some(&scratch.socket))
                .expect("query dead OMP pane")
                .is_none()
        },
        || scratch.screen("omp-live-b"),
    );
    wait(
        "dead OMP pane inherited shared lookup",
        || {
            boop::live::session_in_pane(&registry, &pane_a, &scratch.mail())
                .expect("dispatch dead OMP pane through inherited TMUX")
                .is_none()
        },
        || scratch.screen("omp-live-b"),
    );
    wait(
        "dead OMP pane shared lookup",
        || {
            session_in_pane_on_socket(&registry, &pane_a, Some(&scratch.socket), &scratch.mail())
                .expect("dispatch dead OMP pane")
                .is_none()
        },
        || scratch.screen("omp-live-b"),
    );

    let resume_args = vec!["--resume".to_owned(), session_a.clone()];
    launch_omp(
        &scratch,
        &registry,
        &executable,
        provider.port,
        "omp-live-resumed",
        "omp-live-resumed",
        &resume_args,
    );
    let resumed_pane = scratch.pane("omp-live-resumed");
    assert_eq!(route_session(&scratch, "omp-live-resumed"), session_a);
    assert_eq!(
        session_in_pane_on_socket(
            &registry,
            &resumed_pane,
            Some(&scratch.socket),
            &scratch.mail(),
        )
        .expect("Registry dispatch after OMP resume")
        .as_deref(),
        Some(session_a.as_str())
    );
}
