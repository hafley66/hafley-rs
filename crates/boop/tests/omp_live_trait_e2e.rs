//! OMP's real terminal records bind a live tmux pane to the transcript UUID
//! that Instant attribution reads. This receipt runs two real OMP TUIs through
//! `boop tui omp`, against llmock, on a test-owned tmux socket and git repo.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use boop::harness::mock_tui::{self, MockTuiContext, MockTuiReplay};
use boop::harness::{shell_quote, HarnessId};
use boop::live::session_in_pane_on_socket;
use boop::Registry;
use boop_store::ident::{Store, TurnQuery};

const BOOP: &str = env!("CARGO_BIN_EXE_boop");
const POLL: Duration = Duration::from_millis(100);
const DEADLINE: Duration = Duration::from_secs(75);

struct Scratch {
    root: PathBuf,
    socket: String,
}

impl Scratch {
    fn new() -> Self {
        let unique = format!("{}-{}", std::process::id(), boop::live::now_ms());
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
    fn set(key: &'static str, value: &Path) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: this standalone integration target contains one test body.
        unsafe { std::env::set_var(key, value) };
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        // SAFETY: restores the single process-global variable this test owns.
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
    let path = std::env::var("PATH").expect("PATH for installed omp launcher");
    let mut command = format!(
        "exec env -i PATH={} HOME={} TMPDIR={} TERM=xterm-256color XDG_CONFIG_HOME={} XDG_DATA_HOME={} XDG_CACHE_HOME={} XDG_STATE_HOME={} PI_CODING_AGENT_DIR={} BOOP_MAIL_DIR={} BOOP_DB={} BOOP_NO_SYNC=1 BOOP_NATIVE_PROJECT_EVERY_MS=100 BOOP_NATIVE_DISCOVER_EVERY_MS=100 OMP_SKIP_SETUP=1 TMUX=\"$TMUX\" TMUX_PANE=\"$TMUX_PANE\" {} tui omp --name {} --bin {} --cwd {} --mail-dir {} --",
        shell_quote(&path),
        shell_quote(&scratch.home().display().to_string()),
        shell_quote(&scratch.tmp().display().to_string()),
        shell_quote(&scratch.home().join(".config").display().to_string()),
        shell_quote(&scratch.home().join(".local/share").display().to_string()),
        shell_quote(&scratch.home().join(".cache").display().to_string()),
        shell_quote(&scratch.home().join(".local/state").display().to_string()),
        shell_quote(&scratch.agent_dir().display().to_string()),
        shell_quote(&scratch.mail().display().to_string()),
        shell_quote(&scratch.db().display().to_string()),
        shell_quote(BOOP),
        shell_quote(route),
        shell_quote(&executable.display().to_string()),
        shell_quote(&scratch.repo().display().to_string()),
        shell_quote(&scratch.mail().display().to_string()),
    );
    for arg in &launch.args {
        command.push(' ');
        command.push_str(&shell_quote(arg));
    }
    for arg in extra_args {
        command.push(' ');
        command.push_str(&shell_quote(arg));
    }
    command.push_str(" --auto-approve");
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
    let Some(llmock) = mock_tui::resolve_llmock() else {
        eprintln!("skip omp_live_trait_e2e: no llmock (set LLMOCK_BIN)");
        return;
    };
    let Some(executable) = mock_tui::resolve_executable("omp", "OMP_BIN") else {
        eprintln!("skip omp_live_trait_e2e: no omp executable (set OMP_BIN)");
        return;
    };
    let scratch = Scratch::new();
    let _home = EnvGuard::set("HOME", &scratch.home());
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
