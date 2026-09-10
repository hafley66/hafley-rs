//! Almost-E2E live harness: the real installed third-party CLI, driven through
//! a real PTY against a local mocked provider, with the harness's own native
//! transcript then decoded by the real Boop adapter.
//!
//! Process graph, one adapter at a time:
//!
//! ```text
//! authored provider fixture (tests/fixtures/provider/0_terminal-flow.yaml)
//!   -> pinned local llmock on 127.0.0.1
//!   -> isolated HOME/config (the launch table below)
//!   -> real installed harness under a real PTY (purchased tui-test-rs)
//!   -> asciicast byte evidence + rendered grid
//!   -> harness-authored native transcript file / SQLite
//!   -> real boop_harness::Harness::read_from
//!   -> normalized AgentEvent assertions
//! ```
//!
//! Clocks are deliberately not unified. The cast carries PTY-relative
//! milliseconds, the native transcript carries the harness's wall-clock
//! timestamps, and the provider fixture's `ttft_ms` is a provider-side stream
//! delay. No assertion here claims a shared clock across planes.
//!
//! The whole run is a bought toolchain: `llmock` for the provider protocol and
//! `tui-test-rs` for the PTY and terminal emulation. Nothing here hand-rolls a
//! PTY, a terminal, or a provider wire format. Ordinary unit tests never reach
//! the live body: it is `#[ignore]`d and runs only through
//! `just boop-live-harness`.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use boop::Registry;
use boop_harness::harness::replay::{ReplayChannel, parse_cast};
use boop_harness::{Harness, HarnessId, SessionRef};
use tui_test::{
    AutomaticRecording, KeyAction, LocatorExpectOptions, OpenOptions, Operation, OperationResult,
    RunOptions, Session,
};

// ---------------------------------------------------------------------------
// Launch table: one uniform model over the four adapters.
// ---------------------------------------------------------------------------

/// The one user turn every adapter is driven with.
const LIVE_PROMPT: &str = "render the terminal flow";
/// The fixed assistant marker the authored provider fixture replies with. The
/// provider stream is fixed, so the marker is a determinism anchor, not a model
/// output.
const LIVE_REPLY_MARKER: &str = "FIXED_TERMINAL_REPLY";
/// The provider-local model name each isolated config points at.
const LIVE_PROVIDER_MODEL: &str = "mock-model";

/// How the driver delivers the one user turn once the TUI is up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LiveSubmit {
    /// The prompt already rides the launch arguments; the driver only records.
    InArgument,
    /// The driver types the prompt and presses Enter after readiness.
    TypePrompt,
}

/// One adapter's launch intent against the local provider.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LiveHarnessLaunch {
    program: PathBuf,
    args: Vec<String>,
    /// Environment overrides only. A PTY driver spawns over the inherited
    /// environment, so these are the isolated HOME/config/provider values.
    env: Vec<(String, String)>,
    cwd: PathBuf,
    /// Every file this launch wrote, all under `home`.
    config_paths: Vec<PathBuf>,
    submit: LiveSubmit,
}

fn executable_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

/// The installed executable for one adapter, or `None` when it is absent. An
/// explicit `*_BIN` override wins over `PATH`.
fn resolve_executable(id: HarnessId) -> Option<PathBuf> {
    let (name, override_var) = match id {
        HarnessId::Claude => ("claude", "CLAUDE_BIN"),
        HarnessId::Codex => ("codex", "CODEX_BIN"),
        HarnessId::Kimi => ("kimi", "KIMI_BIN"),
        HarnessId::Opencode => ("opencode", "OPENCODE_BIN"),
    };
    if let Some(value) = std::env::var_os(override_var).filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(value));
    }
    executable_on_path(name)
}

/// The environment every isolated launch shares: a private HOME and private
/// XDG roots so no adapter can read or write the caller's real one.
fn common_env(home: &Path) -> Vec<(String, String)> {
    let home = home.display().to_string();
    vec![
        ("HOME".into(), home.clone()),
        ("TERM".into(), "xterm-256color".into()),
        ("XDG_CONFIG_HOME".into(), format!("{home}/.config")),
        ("XDG_DATA_HOME".into(), format!("{home}/.local/share")),
        ("XDG_CACHE_HOME".into(), format!("{home}/.cache")),
        ("XDG_STATE_HOME".into(), format!("{home}/.local/state")),
    ]
}

fn write(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create config dir {}", parent.display()))?;
    }
    std::fs::write(path, contents).with_context(|| format!("write config {}", path.display()))
}

/// Write one adapter's isolated config under `home` and return its launch.
///
/// `port` is the loopback port the authored provider fixture is served on.
fn launch(
    id: HarnessId,
    program: PathBuf,
    home: &Path,
    workspace: &Path,
    port: u16,
) -> Result<LiveHarnessLaunch> {
    std::fs::create_dir_all(home).with_context(|| format!("create home {}", home.display()))?;
    std::fs::create_dir_all(workspace)
        .with_context(|| format!("create workspace {}", workspace.display()))?;
    // macOS temp paths are symlinked (`/var` -> `/private/var`); the harness
    // canonicalizes cwd before checking trust, so the seeded project key has to
    // be the canonical path or the trust dialog reappears per run.
    let workspace = std::fs::canonicalize(workspace)
        .with_context(|| format!("canonicalize workspace {}", workspace.display()))?;
    let workspace_name = workspace
        .to_str()
        .context("workspace path is not utf-8")?
        .to_owned();
    let mut env = common_env(home);
    let (args, config_paths, submit) = match id {
        HarnessId::Codex => codex(home, &workspace_name, port, &mut env)?,
        HarnessId::Claude => claude(home, &workspace_name, port, &mut env)?,
        HarnessId::Kimi => kimi(home, port, &mut env)?,
        HarnessId::Opencode => opencode(home, port, &mut env)?,
    };
    Ok(LiveHarnessLaunch {
        program,
        args,
        env,
        cwd: workspace.to_path_buf(),
        config_paths,
        submit,
    })
}

type LaunchParts = (Vec<String>, Vec<PathBuf>, LiveSubmit);

fn codex(
    home: &Path,
    workspace: &str,
    port: u16,
    env: &mut Vec<(String, String)>,
) -> Result<LaunchParts> {
    let codex_home = home.join(".codex");
    let config = codex_home.join("config.toml");
    write(
        &config,
        &[
            format!("model = {LIVE_PROVIDER_MODEL:?}"),
            "model_provider = \"llmock\"".into(),
            "approval_policy = \"never\"".into(),
            "sandbox_mode = \"read-only\"".into(),
            "disable_response_storage = true".into(),
            format!("[projects.{}]", serde_json::to_string(workspace)?),
            "trust_level = \"trusted\"".into(),
            "[model_providers.llmock]".into(),
            "name = \"llmock\"".into(),
            format!("base_url = \"http://127.0.0.1:{port}/openai/v1\""),
            "wire_api = \"responses\"".into(),
            "env_key = \"OPENAI_API_KEY\"".into(),
            "requires_openai_auth = false".into(),
            "request_max_retries = 0".into(),
            "stream_max_retries = 0".into(),
            "supports_websockets = false".into(),
            String::new(),
        ]
        .join("\n"),
    )?;
    env.push(("CODEX_HOME".into(), codex_home.display().to_string()));
    env.push(("OPENAI_API_KEY".into(), "test".into()));
    Ok((
        vec![
            "--no-alt-screen".into(),
            "-C".into(),
            workspace.to_owned(),
            LIVE_PROMPT.into(),
        ],
        vec![config],
        LiveSubmit::InArgument,
    ))
}

fn claude(
    home: &Path,
    workspace: &str,
    port: u16,
    env: &mut Vec<(String, String)>,
) -> Result<LaunchParts> {
    let config_dir = home.join(".claude");
    // A normal install keeps global config in `~/.claude.json` and per-project
    // state in `~/.claude/`. Isolating HOME lands both here; setting
    // CLAUDE_CONFIG_DIR instead split the trust write from the trust read and
    // the dialog reappeared after claude's post-onboarding relaunch.
    let state = home.join(".claude.json");
    write(
        &state,
        &serde_json::to_string_pretty(&serde_json::json!({
            "firstStartTime": "2026-01-01T00:00:00.000Z",
            "firstStartVersion": "2",
            "hasCompletedOnboarding": true,
            "lastOnboardingVersion": "999.0.0",
            "projects": {
                workspace: { "hasTrustDialogAccepted": true }
            },
        }))?,
    )?;
    let settings = config_dir.join("settings.json");
    write(
        &settings,
        &serde_json::to_string_pretty(&serde_json::json!({"theme": "dark"}))?,
    )?;
    env.push((
        "ANTHROPIC_BASE_URL".into(),
        format!("http://127.0.0.1:{port}/anthropic"),
    ));
    env.push(("ANTHROPIC_AUTH_TOKEN".into(), "test".into()));
    env.push(("DISABLE_AUTOUPDATER".into(), "1".into()));
    env.push(("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".into(), "1".into()));
    Ok((
        vec![
            "--bare".into(),
            "--model".into(),
            "claude-sonnet-4-5".into(),
            "--permission-mode".into(),
            "dontAsk".into(),
        ],
        vec![state, settings],
        LiveSubmit::TypePrompt,
    ))
}

fn kimi(home: &Path, port: u16, _env: &mut Vec<(String, String)>) -> Result<LaunchParts> {
    let config = home.join(".kimi-code").join("config.toml");
    write(
        &config,
        &[
            format!("default_model = \"llmock/{LIVE_PROVIDER_MODEL}\""),
            "telemetry = false".into(),
            "[providers.llmock]".into(),
            "type = \"openai\"".into(),
            "api_key = \"test\"".into(),
            format!("base_url = \"http://127.0.0.1:{port}/openai/v1\""),
            format!("[models.\"llmock/{LIVE_PROVIDER_MODEL}\"]"),
            "provider = \"llmock\"".into(),
            format!("model = \"{LIVE_PROVIDER_MODEL}\""),
            "max_context_size = 100000".into(),
            "capabilities = [\"tool_call\"]".into(),
            "display_name = \"Mock Model\"".into(),
            String::new(),
        ]
        .join("\n"),
    )?;
    Ok((
        vec![
            "--model".into(),
            format!("llmock/{LIVE_PROVIDER_MODEL}"),
            "--prompt".into(),
            LIVE_PROMPT.into(),
            "--output-format".into(),
            "text".into(),
        ],
        vec![config],
        LiveSubmit::InArgument,
    ))
}

fn opencode(home: &Path, port: u16, env: &mut Vec<(String, String)>) -> Result<LaunchParts> {
    let config = home.join("opencode.json");
    write(
        &config,
        &serde_json::to_string_pretty(&serde_json::json!({
            "$schema": "https://opencode.ai/config.json",
            "autoupdate": false,
            "model": format!("llmock/{LIVE_PROVIDER_MODEL}"),
            "provider": {
                "llmock": {
                    "npm": "@ai-sdk/openai-compatible",
                    "name": "llmock",
                    "options": {
                        "baseURL": format!("http://127.0.0.1:{port}/openai/v1"),
                        "apiKey": "test",
                    },
                    "models": { LIVE_PROVIDER_MODEL: { "name": "Mock Model" } },
                }
            },
        }))?,
    )?;
    env.push(("OPENCODE_CONFIG".into(), config.display().to_string()));
    env.push(("OPENCODE_DISABLE_AUTOUPDATE".into(), "1".into()));
    Ok((
        vec![
            "run".into(),
            "--pure".into(),
            "--interactive".into(),
            "--auto".into(),
            "--model".into(),
            format!("llmock/{LIVE_PROVIDER_MODEL}"),
            LIVE_PROMPT.into(),
        ],
        vec![config],
        LiveSubmit::InArgument,
    ))
}

// ---------------------------------------------------------------------------
// Offline guards: run always, no binary, no network.
// ---------------------------------------------------------------------------

const PROVIDER_FIXTURE: &str = "tests/fixtures/provider/0_terminal-flow.yaml";

/// The authored provider fixture is the one source of the fixed reply. This
/// offline guard fails before any live run if the two drift apart.
#[test]
fn the_authored_fixture_carries_the_fixed_reply() {
    let fixture = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(PROVIDER_FIXTURE),
    )
    .expect("authored provider fixture is committed");
    assert!(fixture.contains(LIVE_REPLY_MARKER));
    assert!(fixture.contains("prompt_tokens: 12"));
}

/// RECEIPT. Every adapter's launch is written entirely under the supplied home,
/// names only the loopback provider port, and carries no external provider
/// destination. Sabotage: a config written to the caller's home or an external
/// provider URL fails here before any binary runs.
#[test]
fn every_launch_isolates_config_and_names_only_loopback() {
    let port = 41234;
    for id in HarnessId::ALL {
        let root = std::env::temp_dir().join(format!(
            "boop-live-launch-{}-{}",
            id.as_str(),
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        let workspace = root.join("workspace");
        let launch = launch(id, PathBuf::from("probe"), &home, &workspace, port)
            .expect("launch writes isolated config");

        assert_eq!(launch.cwd, std::fs::canonicalize(&workspace).unwrap());
        assert!(!launch.config_paths.is_empty());
        for path in &launch.config_paths {
            assert!(
                path.starts_with(&home),
                "{id} wrote config outside its home: {}",
                path.display()
            );
            assert!(path.is_file(), "{id} config was not written");
            let text = std::fs::read_to_string(path).unwrap();
            for external in [
                "api.openai.com",
                "api.anthropic.com",
                "generativelanguage",
                "openrouter.ai",
            ] {
                assert!(
                    !text.contains(external),
                    "{id} config names external provider {external}: {text}"
                );
            }
        }
        let home_env = launch
            .env
            .iter()
            .find(|(key, _)| key == "HOME")
            .expect("HOME is pinned");
        assert_eq!(home_env.1, home.display().to_string());
        for (key, value) in &launch.env {
            if key.ends_with("BASE_URL") {
                assert!(
                    value.contains(&format!("127.0.0.1:{port}")),
                    "{id} {key} is not loopback: {value}"
                );
            }
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}

/// claude alone types its turn; the other three pass it as an argument.
#[test]
fn the_one_turn_submission_shape_is_fixed() {
    let root = std::env::temp_dir().join(format!("boop-live-submit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for id in HarnessId::ALL {
        let launch = launch(
            id,
            PathBuf::from("probe"),
            &root.join(id.as_str()),
            &root.join("workspace"),
            1,
        )
        .unwrap();
        let expected = if id == HarnessId::Claude {
            LiveSubmit::TypePrompt
        } else {
            LiveSubmit::InArgument
        };
        assert_eq!(launch.submit, expected, "{id}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Live run: `just boop-live-harness`.
// ---------------------------------------------------------------------------

const LLMOCK_RELATIVE: &str = "live-tools/llmock/bin/llmock";

fn llmock_path() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("LLMOCK_BIN").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(value));
    }
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    let candidate = cache.join("boop").join(LLMOCK_RELATIVE);
    if candidate.is_file() {
        return Some(candidate);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join("llmock"))
        .find(|candidate| candidate.is_file())
}

fn unused_loopback_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

/// The llmock child plus its drained stderr. `Drop` kills it, so a panic or a
/// failed assertion never leaks a listener into the next run.
struct Llmock {
    child: Child,
    log: Option<std::thread::JoinHandle<String>>,
}

impl Llmock {
    fn spawn(program: &Path, fixture: &Path, port: u16) -> Result<Self> {
        let mut child = Command::new(program)
            .args([
                "--port",
                &port.to_string(),
                "--fixtures",
                fixture.to_str().context("fixture path is not utf-8")?,
                "--deterministic",
                "--default-ttft-ms",
                "0",
                "--default-inter-token-ms",
                "0",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("spawn llmock {}", program.display()))?;
        // Drain stderr while the run proceeds: a full pipe would block llmock.
        let log = child.stderr.take().map(|mut stderr| {
            std::thread::spawn(move || {
                let mut log = String::new();
                let _ = Read::read_to_string(&mut stderr, &mut log);
                log
            })
        });
        let mut server = Self { child, log };
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if let Some(status) = server.child.try_wait()? {
                bail!("llmock exited before binding 127.0.0.1:{port}: {status}");
            }
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return Ok(server);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        bail!("llmock did not bind 127.0.0.1:{port}")
    }

    fn log(&mut self) -> String {
        // Close the pipe before joining the drain thread, or the join blocks
        // on a live child that still holds stderr open.
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.log
            .take()
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default()
    }
}

impl Drop for Llmock {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn discover_session(adapter: &dyn Harness, home: &Path) -> Result<SessionRef> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(sessions) = adapter.sessions() {
            if let Some(session) = sessions
                .into_iter()
                .find(|session| session.path.starts_with(home))
            {
                return Ok(session);
            }
        }
        if Instant::now() >= deadline {
            bail!(
                "{} wrote no native transcript under {} within 30s",
                adapter.id(),
                home.display()
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// One adapter's live result, printed as its receipt line.
struct AdapterReport {
    events: usize,
    skipped: usize,
    cast_events: usize,
}

fn press(session: &Session, key: &str) -> Result<()> {
    session.execute(Operation::Key {
        keys: vec![key.to_owned()],
        action: KeyAction::Press,
    })?;
    Ok(())
}

/// Two first-run screens can precede claude's composer. Accept each default:
/// the folder-trust dialog (move to "Yes, I trust this folder", Enter) and the
/// theme picker (Enter). Seed data covers the common case; this makes the run
/// independent of whether the harness honors that seed.
fn settle_claude_onboarding(session: &Session) -> Result<()> {
    let visible = |text: &str, timeout: u64| {
        session
            .get_by_text(text)
            .expect_with(LocatorExpectOptions {
                not: false,
                timeout_ms: Some(timeout),
            })
            .is_ok()
    };
    for attempt in 0..6 {
        let timeout = if attempt == 0 { 6_000 } else { 1_500 };
        if visible("Quick safety check", timeout) {
            press(session, "down")?;
            press(session, "enter")?;
            continue;
        }
        if visible("Choose the text style", timeout) {
            press(session, "enter")?;
            continue;
        }
        if visible("Security notes", timeout) {
            press(session, "enter")?;
            continue;
        }
        return Ok(());
    }
    Ok(())
}

fn run_adapter(id: HarnessId, port: u16) -> Result<AdapterReport> {
    let Some(program) = resolve_executable(id) else {
        bail!("{id} executable is absent from PATH and its *_BIN override is unset");
    };
    let root = tempfile::tempdir().context("create live adapter root")?;
    let home = root.path().join("home");
    let workspace = root.path().join("workspace");
    let casts = root.path().join("casts");
    let launch = launch(id, program, &home, &workspace, port)?;

    let defaults = OpenOptions::default();
    let session = Session::new(format!("boop-live-{id}-{}", std::process::id()));
    session
        .run(RunOptions {
            backend: defaults.backend,
            program: launch.program.display().to_string(),
            args: launch.args.clone(),
            profile: defaults.profile,
            cols: 120,
            rows: 35,
            cwd: Some(launch.cwd.display().to_string()),
            env: launch.env.clone(),
            wait_ready: Some(false),
            restart: false,
            timeouts: defaults.timeouts,
            recording: AutomaticRecording {
                directory: Some(casts),
                ..Default::default()
            },
        })
        .with_context(|| format!("{id} failed to start under the PTY"))?;

    if launch.submit == LiveSubmit::TypePrompt {
        settle_claude_onboarding(&session)?;
        // The composer is up once the versioned banner is on screen; typing
        // before it would be lost.
        session
            .get_by_text("Claude Code v")
            .expect_with(LocatorExpectOptions {
                not: false,
                timeout_ms: Some(90_000),
            })
            .with_context(|| format!("{id} TUI never became ready"))?;
        session
            .execute(Operation::Submit {
                data: Some(LIVE_PROMPT.to_string()),
            })
            .with_context(|| format!("{id} rejected the typed prompt"))?;
    }

    if let Err(error) = session
        .get_by_text(LIVE_REPLY_MARKER)
        .expect_with(LocatorExpectOptions {
            not: false,
            timeout_ms: Some(120_000),
        })
    {
        let cast = session.recording().unwrap_or_default();
        let tail: String = cast
            .chars()
            .rev()
            .take(4_000)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        bail!("{id} never rendered the fixed reply marker: {error}\ncast tail:\n{tail}");
    }

    let screen = match session.execute(Operation::Text { full: true })? {
        OperationResult::Text(text) => text,
        other => bail!("{id} screen read returned {other:?}"),
    };
    assert!(
        screen.contains(LIVE_REPLY_MARKER),
        "{id} grid lost the fixed reply marker"
    );

    let cast = session.recording().with_context(|| {
        format!("{id} produced no asciicast evidence; is automatic recording on?")
    })?;
    let schedule = parse_cast(&cast).with_context(|| format!("{id} cast did not parse"))?;
    let rendered: String = schedule
        .iter()
        .filter(|event| matches!(event.channel, ReplayChannel::Output))
        .map(|event| String::from_utf8_lossy(&event.data))
        .collect();
    assert!(
        rendered.contains(LIVE_REPLY_MARKER),
        "{id} cast bytes lost the fixed reply marker"
    );
    assert!(
        schedule.windows(2).all(|pair| pair[0].at_ms <= pair[1].at_ms),
        "{id} cast timestamps are not monotonic"
    );

    // Let the turn finish writing before the PTY closes: an interactive
    // harness (opencode) persists the assistant record after output goes idle,
    // and a hard close can race that write.
    let _ = session.execute(Operation::WaitIdle {
        timeout_ms: Some(30_000),
    });
    session.close().ok();
    drop(session);

    // The harness now owns a native transcript under the isolated home. Point
    // the real Boop readers there and decode it.
    let previous = std::env::var_os("BOOP_READER_HOME");
    std::env::set_var("BOOP_READER_HOME", &home);
    let decoded = (|| -> Result<AdapterReport> {
        let registry = Registry::discover();
        let adapter = registry.get(id);
        let session = discover_session(adapter, &home)?;
        // The harness may append its assistant record a beat after the reply
        // lands on the grid; poll the real adapter until the turn is complete.
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut chunk = adapter
            .read_from(&session, 0)
            .with_context(|| format!("{id} read_from rejected its own transcript"))?;
        while chunk.events.len() < 2 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(200));
            chunk = adapter
                .read_from(&session, 0)
                .with_context(|| format!("{id} read_from rejected its own transcript"))?;
        }
        assert_eq!(chunk.skipped, 0, "{id} adapter skipped records");
        assert!(!chunk.reset, "{id} adapter reset a fresh transcript");
        assert!(!chunk.events.is_empty(), "{id} transcript decoded no events");
        let offsets: Vec<u64> = chunk.events.iter().map(|e| e.raw_line_offset).collect();
        assert!(
            offsets.windows(2).all(|pair| pair[0] < pair[1]),
            "{id} event order is not strictly increasing: {offsets:?}"
        );
        let kinds: Vec<&str> = chunk
            .events
            .iter()
            .map(|event| event.record_type.as_str())
            .collect();
        assert!(
            kinds.len() >= 2 && kinds.first() != kinds.last(),
            "{id} one-turn transcript did not keep distinct event kinds: {kinds:?}"
        );

        // Cursor resume: a second read at the returned offset is empty and does
        // not move. This is the incremental-read contract on real bytes.
        let resumed = adapter
            .read_from(&session, chunk.next_offset)
            .context("resume at cursor")?;
        assert!(
            resumed.events.is_empty(),
            "{id} resume re-decoded {} events",
            resumed.events.len()
        );
        assert_eq!(resumed.next_offset, chunk.next_offset);

        // The fixed reply must have reached the adapter's own turn reader, not
        // just the screen: the native transcript is the proof, not the grid.
        let messages = adapter.messages(&session, None);
        let user = messages
            .iter()
            .position(|message| matches!(message.role.as_str(), "user" | "meta"));
        let assistant = messages.iter().position(|message| {
            message.role == "assistant" && message.text.contains(LIVE_REPLY_MARKER)
        });
        let assistant = assistant
            .with_context(|| format!("{id} adapter turn reader never saw the fixed reply marker"))?;
        assert!(
            user.is_some_and(|user| user < assistant),
            "{id} assistant turn did not follow the user turn"
        );

        Ok(AdapterReport {
            events: chunk.events.len(),
            skipped: chunk.skipped,
            cast_events: schedule.len(),
        })
    })();
    match previous {
        Some(value) => std::env::set_var("BOOP_READER_HOME", value),
        None => std::env::remove_var("BOOP_READER_HOME"),
    }
    decoded
}

/// The adapters a live run exercises. `BOOP_LIVE_ADAPTER=codex` narrows it to
/// one, which is how a single-adapter failure is reproduced without paying for
/// the other three.
fn selected_adapters() -> Result<Vec<HarnessId>> {
    match std::env::var("BOOP_LIVE_ADAPTER")
        .ok()
        .filter(|value| !value.is_empty())
    {
        Some(value) => Ok(vec![HarnessId::parse(&value)
            .with_context(|| format!("unknown BOOP_LIVE_ADAPTER `{value}`"))?]),
        None => Ok(HarnessId::ALL.to_vec()),
    }
}

/// Live only: `just boop-live-harness`. Fails clearly when the pinned toolchain
/// is not installed; skips an adapter only when its executable is absent.
#[test]
#[ignore = "live: installed harness + pinned llmock + real PTY; run just boop-live-harness"]
fn live_harness_almost_e2e() -> Result<()> {
    let llmock = llmock_path().context(
        "pinned llmock is missing: run `just boop-live-setup` (or set LLMOCK_BIN)",
    )?;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join(PROVIDER_FIXTURE);
    let port = unused_loopback_port()?;
    let mut server = Llmock::spawn(&llmock, &fixture, port)?;

    let mut reports = Vec::new();
    let mut skipped = Vec::new();
    let mut failure = None;
    for id in selected_adapters()? {
        match run_adapter(id, port) {
            Ok(report) => {
                eprintln!(
                    "live {id}: {} events, {} skipped, {} cast events",
                    report.events, report.skipped, report.cast_events
                );
                reports.push(report);
            }
            Err(error) => {
                let missing = error.to_string().contains("executable is absent");
                if missing {
                    eprintln!("skip {id}: {error}");
                    skipped.push(id);
                } else {
                    failure = Some(error);
                    break;
                }
            }
        }
    }

    let log = server.log();
    if let Some(error) = failure {
        bail!("live harness failed: {error:#}\nllmock:\n{log}");
    }
    assert!(
        !reports.is_empty(),
        "no adapter ran; skipped: {skipped:?}; llmock:\n{log}"
    );
    eprintln!(
        "live harness: ran {}, skipped {} ({skipped:?})",
        reports.len(),
        skipped.len()
    );
    let _ = std::io::stdout().flush();
    Ok(())
}
