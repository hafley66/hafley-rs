//! The oh-my-pi (`omp`) adapter. omp writes one session file per conversation
//! under `$PI_CODING_AGENT_DIR/sessions` (default `~/.omp/agent/sessions`) in a
//! `<encoded cwd>/<timestamp>_<id>.jsonl` file whose header is the `type ==
//! "session"` line, not line 1; the transcript readers land in a later lane, so
//! this skeleton lists no sessions and reads no bytes.

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::harness::{
    Capabilities, ControlCapabilities, Harness, HarnessId, LanePolicy, MailPolicy, OneShotSpec,
    ReadChunk, SessionRef, SpawnSpec, VariantSupport,
};

pub struct Omp;

static CAPABILITIES: Capabilities = Capabilities {
    bans_plan_family_models: true,
    lanes: LanePolicy::Allowed,
    variant: VariantSupport::Flag,
    mail: MailPolicy::Keystrokes,
    image_paste_keys: None,
    interrupt_keys: Some("Escape"),
    native_tui_projector: true,
    wrapper_owns_alternate_screen: false,
    native_backend: super::NativeBackendSupport::Unsupported,
    native_settings: super::NativeSettingsSupport::Unsupported("omp has no control plane"),
    registry_names_processes: false,
};

/// The `--mode` values that run omp without its TUI; a value outside this set
/// (or no `--mode` at all) leaves the interactive frontend in charge. The
/// `acp` subcommand is separate and headless on its own.
const HEADLESS_MODES: [&str; 3] = ["json", "rpc", "rpc-ui"];

impl Harness for Omp {
    fn id(&self) -> HarnessId {
        HarnessId::Omp
    }

    fn capabilities(&self) -> &'static Capabilities {
        &CAPABILITIES
    }

    fn matches_model(&self, _name: &str) -> bool {
        false
    }

    /// omp runs its TUI unless the invocation is headless: the `acp`
    /// subcommand as the first argument, a `--print`/`-p` one-shot, or a
    /// `--mode` (or `--mode=`) of `json|rpc|rpc-ui`.
    fn uses_native_tui(&self, args: &[String]) -> bool {
        if args.first().is_some_and(|arg| arg == "acp") {
            return false;
        }
        let mut index = 0;
        while index < args.len() {
            let arg = &args[index];
            if arg == "--print" || arg == "-p" {
                return false;
            }
            if arg == "--mode" {
                if let Some(mode) = args.get(index + 1) {
                    if HEADLESS_MODES.contains(&mode.as_str()) {
                        return false;
                    }
                }
            } else if let Some(mode) = arg.strip_prefix("--mode=") {
                if HEADLESS_MODES.contains(&mode) {
                    return false;
                }
            }
            index += 1;
        }
        true
    }

    /// instant's kimi recipe held open: `--print` prints and exits, so the mock
    /// runs the interactive TUI and the driver types. The provider config points
    /// omp at the loopback llmock server.
    fn mock_tui_launch(
        &self,
        ctx: &super::mock_tui::MockTuiContext<'_>,
    ) -> anyhow::Result<super::mock_tui::MockTuiLaunch> {
        use super::mock_tui::{terminal_env, MockTuiReplay};
        let executable = super::mock_tui::resolve_executable("omp", "OMP_BIN")
            .ok_or_else(|| anyhow::anyhow!("no omp executable: set OMP_BIN or put it on PATH"))?;
        let agent_dir = ctx.home.join(".omp").join("agent");
        std::fs::create_dir_all(&agent_dir)?;
        let config = agent_dir.join("models.yml");
        std::fs::write(
            &config,
            [
                "providers:".to_owned(),
                "  llmock:".to_owned(),
                format!("    baseUrl: http://127.0.0.1:{}/openai/v1", ctx.port),
                "    apiKey: test".to_owned(),
                "    api: openai-completions".to_owned(),
                "    models:".to_owned(),
                "      - id: mock-model".to_owned(),
                "        name: Mock Model".to_owned(),
                String::new(),
            ]
            .join("\n"),
        )?;
        let env = terminal_env(ctx.home);
        Ok(super::mock_tui::MockTuiLaunch {
            executable: executable.display().to_string(),
            args: vec!["--model".into(), "llmock/mock-model".into()],
            env,
            config_paths: vec![config],
            replay: MockTuiReplay::TypePrompt {
                readiness: "Mock Model",
            },
        })
    }

    fn sessions(&self) -> Result<Vec<SessionRef>> {
        Ok(Vec::new())
    }

    fn session_roots(&self) -> Result<Vec<PathBuf>> {
        Ok(vec![omp_sessions_dir()?])
    }

    fn read_from(&self, _session: &SessionRef, offset: u64) -> Result<ReadChunk> {
        Ok(ReadChunk {
            events: Vec::new(),
            next_offset: offset,
            reset: false,
            skipped: 0,
        })
    }

    fn preview_command(&self, spec: &SpawnSpec) -> Option<String> {
        Some(spawn_command(spec))
    }

    fn spawn(&self, spec: &SpawnSpec) -> Result<SessionRef> {
        let tmux_name = spec
            .tmux
            .clone()
            .unwrap_or_else(|| format!("boop-{}", spec.lane));
        let cwd = crate::worktree::prepare_spawn_dir(spec)?;
        let command = spawn_command(spec);
        boop_store::tmux::mux().new_detached_session(
            spec.socket.as_deref(),
            &tmux_name,
            &cwd.display().to_string(),
            &command,
        )?;
        Ok(SessionRef {
            harness: HarnessId::Omp,
            session_id: spec.lane.clone(),
            nickname: spec.lane.clone(),
            path: omp_sessions_dir().unwrap_or_else(|_| cwd.join(".omp-sessions")),
            cwd: Some(cwd.display().to_string()),
            git_branch: Some(spec.branch.clone()),
            modified_ms: boop_acp::channel::now_ms(),
            size: 0,
            tmux: Some(tmux_name),
            tmux_socket: spec.socket.clone(),
            parent: None,
        })
    }

    fn stop(&self, session: &SessionRef) -> Result<()> {
        if let Some(tmux) = &session.tmux {
            if boop_store::tmux::mux().has_session(session.tmux_socket.as_deref(), tmux)? {
                boop_store::tmux::mux().kill_session(session.tmux_socket.as_deref(), tmux)?;
            }
        }
        Ok(())
    }

    fn one_shot(&self, spec: &OneShotSpec) -> Result<String> {
        let mut command = std::process::Command::new("omp");
        command.args(["--print", "--allow-home", "--no-session"]);
        if let Some(model) = spec.model.as_deref().filter(|value| !value.is_empty()) {
            command.args(["--model", model]);
        }
        command.arg(&spec.prompt);
        let output = command.output().context("spawn omp --print")?;
        if !output.status.success() {
            anyhow::bail!(
                "omp --print failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    fn open_channel(
        &self,
        spec: &boop_acp::channel::ChannelSpec,
    ) -> anyhow::Result<Box<dyn boop_acp::channel::LaneChannel>> {
        Ok(Box::new(boop_acp::channel::acp::AcpChannel::open_adapter(
            spec,
            boop_acp::channel::acp::OMP_ADAPTER,
        )?))
    }

    /// `send_midflight` stays false: ACP takes one `session/prompt` per turn.
    fn control_capabilities(&self) -> ControlCapabilities {
        ControlCapabilities {
            send_midflight: false,
            resume: true,
            spawn: true,
            subagent_visible: false,
        }
    }
}

/// omp's session root: `$PI_CODING_AGENT_DIR` when set, else `~/.omp/agent`,
/// under `sessions`.
fn omp_sessions_dir() -> Result<PathBuf> {
    let root = match std::env::var_os("PI_CODING_AGENT_DIR").filter(|value| !value.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("resolve omp agent dir"))?
            .join(".omp")
            .join("agent"),
    };
    Ok(root.join("sessions"))
}

/// The command a supervised omp lane pane runs: the raw omp TUI, pinned to the
/// worktree with `--allow-home` (omp would otherwise auto-switch to a temp
/// cwd), opened on the brief as its first `@file` message.
fn spawn_command(spec: &SpawnSpec) -> String {
    let mut command = "omp --allow-home".to_owned();
    if let Some(model) = spec.model.as_deref().filter(|value| !value.is_empty()) {
        command.push_str(&format!(" --model {}", crate::harness::shell_quote(model)));
    }
    command.push_str(&format!(" @{}", crate::harness::shell_quote(&spec.prompt)));
    command
}
