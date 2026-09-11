//! Real-TUI scream integration: each harness CLI runs against a loopback
//! llmock provider (its `mock_tui_launch` recipe), registers through
//! `boop tui`, renders the canned reply, then takes a `boop beep scream`.
//!
//! Skips per harness when the CLI or `llmock` is absent:
//!   cargo install --git https://github.com/larsakerlund/llmock.git \
//!     --tag v0.1.2 --locked llmock
//! Executable overrides: CODEX_BIN, CLAUDE_BIN (ccz rides this),
//! OPENCODE_BIN, KIMI_BIN, LLMOCK_BIN.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use boop::harness::mock_tui::{self, MockTuiReplay};
use boop::harness::{shell_quote, HarnessId};
use boop::Registry;

const BOOP: &str = env!("CARGO_BIN_EXE_boop");

/// One harness's place in the matrix.
struct Case {
    entry: &'static str,
    id: HarnessId,
    executable_override: &'static str,
}

const CASES: &[Case] = &[
    Case {
        entry: "codex",
        id: HarnessId::Codex,
        executable_override: "CODEX_BIN",
    },
    Case {
        entry: "claude",
        id: HarnessId::Claude,
        executable_override: "CLAUDE_BIN",
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

/// Scratch root for one case: home, workspace, mailbox, tmux session. The
/// session rides the DEFAULT server (coordinator_ping's TestSession shape)
/// because route liveness reads no other socket.
struct Scratch {
    root: PathBuf,
    session: String,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = tmux(&["kill-session", "-t", &self.session]);
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

fn wait_for_screen(session: &str, wanted: &str, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let text = screen(session);
        if text.contains(wanted) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{label}: never saw {wanted:?}\n{text}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// The wrapper writes its route once the harness's first session id is
/// observed, which for some harnesses lands only after the first message.
fn wait_for_route(scratch: &Scratch, route: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let rows = boop(
            scratch,
            &[
                "db",
                &format!("SELECT kind FROM agent_route WHERE route = '{route}'"),
            ],
        );
        if String::from_utf8_lossy(&rows.stdout).contains("coordinator") {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "route {route} never registered in the scratch store"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// The scratch-store boop calls: the store lives in the mail dir, so both
/// the mail-dir flag and BOOP_DB name that one file.
fn boop(scratch: &Scratch, args: &[&str]) -> std::process::Output {
    use boop_store::testing::BoopCommandExt;
    Command::new(BOOP)
        .args(args)
        .boop_test_root(&scratch.root)
        .env("BOOP_DB", scratch.root.join("mail").join("boop.db"))
        .env("BOOP_NO_SYNC", "1")
        .output()
        .expect("run boop")
}

/// RECEIPT. A real harness TUI, mock-provisioned by its own adapter recipe,
/// registers as a coordinator; `boop beep scream` presses the interrupt key
/// into its pane and leaves the broadcast row in the scratch store. Sabotage:
/// dropping the key press leaves scream's stdout without `interrupted`.
#[test]
fn scream_interrupts_each_real_tui() {
    let Some(llmock) = mock_tui::resolve_llmock() else {
        eprintln!("skipping: no llmock (cargo install --tag v0.1.2 llmock)");
        return;
    };
    let registry = Registry::discover();
    for case in CASES {
        let tag = format!("boop-shout-{}-{}", case.entry, std::process::id());
        let root =
            std::env::temp_dir().join(format!("boop-shout-{}-{}", case.entry, std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let _ = tmux(&["kill-session", "-t", &tag]);
        std::fs::create_dir_all(root.join("mail")).unwrap();
        let scratch = Scratch {
            session: tag.clone(),
            root: root.clone(),
        };
        let provider = match mock_tui::MockProvider::spawn(&llmock, None) {
            Ok(provider) => provider,
            Err(error) => panic!("llmock spawn: {error}"),
        };
        let adapter = registry.get(case.id);
        std::fs::create_dir_all(root.join("home")).unwrap();
        std::fs::create_dir_all(root.join("workspace")).unwrap();
        let launch = match adapter.mock_tui_launch(&mock_tui::MockTuiContext {
            home: &root.join("home"),
            workspace: &root.join("workspace"),
            port: provider.port,
        }) {
            Ok(launch) => launch,
            Err(error) => {
                eprintln!("skipping {}: {error}", case.entry);
                continue;
            }
        };
        run_tui_in_pane(&scratch, case, &launch, &tag);

        if let MockTuiReplay::TypePrompt { readiness } = launch.replay {
            wait_for_screen(&tag, readiness, case.entry);
            let _ = tmux(&["send-keys", "-t", &tag, "-l", mock_tui::MOCK_PROMPT]);
            std::thread::sleep(Duration::from_millis(250));
            let _ = tmux(&["send-keys", "-t", &tag, "Enter"]);
        }
        wait_for_screen(&tag, mock_tui::MOCK_REPLY_MARKER, case.entry);
        let route = format!("shout-e2e-{}", case.entry);
        wait_for_route(&scratch, &route);

        let screamed = boop(
            &scratch,
            &[
                "beep",
                "scream",
                "--mail-dir",
                &root.join("mail").display().to_string(),
            ],
        );
        let stdout = String::from_utf8_lossy(&screamed.stdout);
        assert!(screamed.status.success(), "{}: {stdout}", case.entry);
        assert!(
            stdout.contains(&format!("interrupted {route} in ")),
            "{}: scream never pressed a key\n{stdout}",
            case.entry
        );
        if case.id == HarnessId::Claude {
            wait_for_screen(&tag, "nterrupted", case.entry);
        }
        let rows = boop(
            &scratch,
            &["db", "SELECT outcome FROM agent_delivery_transition"],
        );
        let rows = String::from_utf8_lossy(&rows.stdout);
        assert!(
            rows.contains("appended"),
            "{}: no delivery row in the scratch store\n{rows}",
            case.entry
        );
    }
}

/// One pane, one wrapped TUI: the recipe env rides an `env` prefix so the
/// wrapper and its harness child both see the scratch home and provider.
fn run_tui_in_pane(scratch: &Scratch, case: &Case, launch: &mock_tui::MockTuiLaunch, tag: &str) {
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
        " {} tui {} --name shout-e2e-{} --bin {} --cwd {} --mail-dir {} --",
        shell_quote(BOOP),
        case.entry,
        case.entry,
        shell_quote(&launch.executable),
        shell_quote(&scratch.root.join("workspace").display().to_string()),
        shell_quote(&scratch.root.join("mail").display().to_string()),
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
        "{}: tmux new-session failed: {}",
        case.entry,
        String::from_utf8_lossy(&output.stderr)
    );
}
