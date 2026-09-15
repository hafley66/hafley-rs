//! The oh-my-pi (`omp`) adapter. omp writes one session file per conversation
//! under `$PI_CODING_AGENT_DIR/sessions` (default `~/.omp/agent/sessions`) in a
//! `<encoded cwd>/<timestamp>_<id>.jsonl` file whose header is the `type ==
//! "session"` line, not line 1.

use std::fs::File;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

use crate::harness::{
    jsonl_files, Capabilities, ControlCapabilities, Harness, HarnessId, Ingested, LanePolicy,
    MailPolicy, OneShotSpec, ReadChunk, SessionRef, SpawnSpec, VariantSupport,
};
use crate::live::{DoorAddress, LiveSession, LiveSessionScope, LiveSessions, LiveStatus};
use boop_store::event::AgentEvent;
use boop_store::ident::{Store, SyncStat, UsageRow};
use boop_store::tail;

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
    // omp's terminal-session record names the exact tmux pane and transcript.
    // It is an identity relation, so the native wrapper never falls back to a
    // cwd/newest-transcript selection for this harness.
    registry_names_processes: true,
};

struct OmpLive;

impl LiveSessions for OmpLive {
    fn live_sessions(&self) -> Result<Vec<LiveSession>> {
        omp_live_sessions_in(&omp_terminal_sessions_dir()?)
    }
}

static LIVE: OmpLive = OmpLive;

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

    fn live(&self) -> &dyn LiveSessions {
        &LIVE
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
        sessions_in(&omp_sessions_dir()?)
    }

    fn session_roots(&self) -> Result<Vec<PathBuf>> {
        Ok(vec![omp_sessions_dir()?])
    }

    fn read_from(&self, session: &SessionRef, offset: u64) -> Result<ReadChunk> {
        let mut file = File::open(&session.path)
            .with_context(|| format!("open transcript {}", session.path.display()))?;
        let result = tail::read_complete_lines(&mut file, offset)?;

        let mut events = Vec::new();
        let mut skipped = 0usize;
        for line in &result.lines {
            match parse_line(session, line) {
                Ok(mut decoded) => events.append(&mut decoded),
                Err(_) => skipped += 1,
            }
        }

        Ok(ReadChunk {
            events,
            next_offset: result.next_offset,
            reset: result.reset,
            skipped,
        })
    }

    fn ingest(&self, store: &Store, session: &SessionRef, from: u64) -> Result<Ingested> {
        let mut file = File::open(&session.path)
            .with_context(|| format!("open transcript {}", session.path.display()))?;
        let result = tail::read_complete_lines(&mut file, from)?;
        if result.lines.is_empty() {
            return Ok(Ingested {
                stat: SyncStat::default(),
                next_cursor: from,
            });
        }
        let mut turn = store.begin_walk(&session.session_id)?;
        let mut stat = SyncStat::default();
        for line in &result.lines {
            project_line(store, session, line, &mut turn, &mut stat)?;
        }
        Ok(Ingested {
            stat,
            next_cursor: result.next_offset,
        })
    }

    fn preview_command(&self, spec: &SpawnSpec) -> Option<String> {
        Some(crate::harness::supervisor_command(spec))
    }

    fn spawn(&self, spec: &SpawnSpec) -> Result<SessionRef> {
        let tmux_name = spec
            .tmux
            .clone()
            .unwrap_or_else(|| format!("boop-{}", spec.lane));
        let cwd = crate::worktree::prepare_spawn_dir(spec)?;
        let command = crate::harness::supervisor_command(spec);
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

    fn describe(&self, session: &SessionRef) -> Option<crate::transcript::SessionMeta> {
        let (input_tokens, model, provider, created) = omp_meta(&session.path);
        Some(crate::transcript::SessionMeta {
            id: session.session_id.clone(),
            harness: HarnessId::Omp,
            cwd: session.cwd.clone().unwrap_or_default(),
            source_path: Some(session.path.to_string_lossy().into_owned()),
            title: None,
            model,
            provider,
            input_tokens,
            parent_id: session.parent.clone(),
            parent_kind: None,
            created_at_ms: created,
            last_activity_ms: session.modified_ms,
        })
    }

    fn messages(
        &self,
        session: &SessionRef,
        after_seq: Option<u64>,
    ) -> Vec<crate::transcript::Message> {
        read_omp(&session.path, &session.session_id, after_seq)
    }

    /// Every omp session is a strip row; there is no main/sub-agent split.
    fn lists_session(&self, _session: &SessionRef) -> bool {
        true
    }

    /// omp resumes on the header uuid, which is the session id.
    fn resume_id<'a>(&self, session: &'a SessionRef) -> &'a str {
        &session.session_id
    }

    fn session_by_id(&self, session_id: &str, _cwd: Option<&str>) -> Option<SessionRef> {
        let sessions = self.sessions().ok()?;
        if let Some(exact) = sessions
            .iter()
            .find(|session| session.session_id == session_id)
        {
            return Some(exact.clone());
        }
        let prefixes: Vec<&SessionRef> = sessions
            .iter()
            .filter(|session| session.session_id.starts_with(session_id))
            .collect();
        (prefixes.len() == 1).then(|| prefixes[0].clone())
    }
}

/// omp's session root: `$PI_CODING_AGENT_DIR` when set, else `~/.omp/agent`,
/// under `sessions`.
fn omp_sessions_dir() -> Result<PathBuf> {
    Ok(omp_agent_dir()?.join("sessions"))
}

/// omp's agent root, shared by transcript and terminal-session discovery.
fn omp_agent_dir() -> Result<PathBuf> {
    Ok(
        match std::env::var_os("PI_CODING_AGENT_DIR").filter(|value| !value.is_empty()) {
            Some(dir) => PathBuf::from(dir),
            None => dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("resolve omp agent dir"))?
                .join(".omp")
                .join("agent"),
        },
    )
}

/// omp writes one exact active-TUI relation per terminal. A tmux record is
/// named `tmux-%<pane>` and contains the launch cwd followed by the active
/// transcript path. The transcript header supplies the active session UUID.
/// `parentSession` is transcript history lineage, not a live TUI parent edge.
fn omp_terminal_sessions_dir() -> Result<PathBuf> {
    Ok(omp_agent_dir()?.join("terminal-sessions"))
}

fn omp_live_sessions_in(base: &Path) -> Result<Vec<LiveSession>> {
    let mut live = Vec::new();
    let entries = match std::fs::read_dir(base) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(live),
        Err(error) => return Err(error).with_context(|| format!("read {}", base.display())),
    };
    for entry in entries {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_file() {
            continue;
        }
        let filename = entry.file_name();
        let Some(pane) = filename
            .to_str()
            .and_then(|name| name.strip_prefix("tmux-"))
            .filter(|pane| pane.starts_with('%') && pane.len() > 1)
        else {
            continue;
        };
        let Ok(record) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let mut lines = record.lines();
        let Some(cwd) = lines.next().filter(|line| !line.is_empty()) else {
            continue;
        };
        let Some(path) = lines.next().filter(|line| !line.is_empty()) else {
            continue;
        };
        let path = PathBuf::from(path);
        let Some(header) = session_header(&path).filter(|header| !header.id.is_empty()) else {
            continue;
        };
        let observed_ms = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|elapsed| elapsed.as_millis() as u64)
            .unwrap_or_else(crate::live::now_ms);
        live.push(LiveSession {
            harness: HarnessId::Omp,
            session_id: header.id,
            pid: None,
            cwd: Some(PathBuf::from(cwd)),
            tmux_pane: Some(pane.to_owned()),
            status: LiveStatus::Unknown,
            door: DoorAddress::None,
            observed_ms,
            started_ms: None,
            scope: LiveSessionScope::Root,
            parent_session: None,
        });
    }
    live.sort_by(|left, right| left.tmux_pane.cmp(&right.tmux_pane));
    Ok(live)
}

// ---- omp transcript reader and shaping.

/// The `type == "session"` header's identity fields; the session header is
/// found by type, never by line index (line 1 is a `title` record).
struct OmpHeader {
    id: String,
    cwd: Option<String>,
    parent: Option<String>,
}

fn session_header(path: &Path) -> Option<OmpHeader> {
    for value in crate::transcript::head_values(path) {
        if value.get("type").and_then(Value::as_str) == Some("session") {
            let id = value
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let cwd = value.get("cwd").and_then(Value::as_str).map(str::to_owned);
            let parent = value
                .get("parentSession")
                .and_then(Value::as_str)
                .map(str::to_owned);
            return Some(OmpHeader { id, cwd, parent });
        }
    }
    None
}

/// The session uuid from a `<timestamp>_<uuid>.jsonl` file stem.
fn file_stem_uuid(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    stem.split_once('_').map(|(_, uuid)| uuid.to_owned())
}

/// One `SessionRef` per `jsonl` under the sessions root. `session_id` is the
/// header id (falling back to the file-stem uuid), `nickname` the file-stem
/// uuid, `cwd` the header cwd, `parent` the header `parentSession`.
fn sessions_in(base: &Path) -> Result<Vec<SessionRef>> {
    let mut sessions = Vec::new();
    for file in jsonl_files(base)? {
        let Some(header) = session_header(&file.path) else {
            continue;
        };
        let uuid = file_stem_uuid(&file.path).unwrap_or_else(|| header.id.clone());
        let session_id = if header.id.is_empty() {
            uuid.clone()
        } else {
            header.id.clone()
        };
        sessions.push(SessionRef {
            harness: HarnessId::Omp,
            session_id,
            nickname: uuid,
            path: file.path,
            cwd: header.cwd,
            git_branch: None,
            modified_ms: file.modified_ms,
            size: file.size,
            tmux: None,
            tmux_socket: None,
            parent: header.parent,
        });
    }
    sessions.sort_by_key(|session| session.modified_ms);
    Ok(sessions)
}

fn build_event(
    session: &SessionRef,
    line: &tail::CompleteLine,
    value: &Value,
    record_type: &str,
    tool_name: Option<String>,
) -> AgentEvent {
    AgentEvent {
        harness: session.harness.as_str(),
        session_id: session.session_id.clone(),
        ts_ms: value
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(boop_store::session::parse_iso_ms)
            .unwrap_or(0),
        uuid: value.get("id").and_then(Value::as_str).map(str::to_owned),
        parent_uuid: value
            .get("parentId")
            .and_then(Value::as_str)
            .map(str::to_owned),
        cwd: session.cwd.clone(),
        git_branch: session.git_branch.clone(),
        record_type: record_type.to_owned(),
        tool_name,
        paths: Vec::new(),
        urls: Vec::new(),
        raw_line_offset: line.start,
    }
}

/// Decode one line into its events. `message` records emit one event each; an
/// assistant message additionally emits one `toolCall` event per tool call.
/// The recognized non-event records (`title`, `session`, `model_change`,
/// `thinking_level_change`, `title_change`, `custom`) and anything not a
/// `message` yield an empty vec. A line that fails to parse as JSON is an
/// error, which the caller counts as skipped.
fn parse_line(session: &SessionRef, line: &tail::CompleteLine) -> Result<Vec<AgentEvent>> {
    let value: Value = serde_json::from_slice(&line.bytes)
        .with_context(|| format!("invalid omp json at byte {}", line.start))?;
    let record_type = value.get("type").and_then(Value::as_str).unwrap_or("");
    if record_type != "message" {
        return Ok(Vec::new());
    }
    let Some(message) = value.get("message") else {
        return Ok(Vec::new());
    };
    let role = message
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("user");
    match role {
        "toolResult" => {
            let name = message
                .get("toolName")
                .and_then(Value::as_str)
                .map(str::to_owned);
            Ok(vec![build_event(session, line, &value, "toolResult", name)])
        }
        "assistant" => {
            let mut events = vec![build_event(session, line, &value, "message", None)];
            if let Some(items) = message.get("content").and_then(Value::as_array) {
                for item in items {
                    if item.get("type").and_then(Value::as_str) == Some("toolCall") {
                        let name = item.get("name").and_then(Value::as_str).map(str::to_owned);
                        events.push(build_event(session, line, &value, "toolCall", name));
                    }
                }
            }
            Ok(events)
        }
        _ => Ok(vec![build_event(session, line, &value, "message", None)]),
    }
}

fn record(stat: &mut SyncStat, inserted: usize) {
    if inserted == 0 {
        stat.dropped += 1;
    } else {
        stat.written += 1;
    }
}

/// The readable text of a message's `content`: a bare string, or the `text`
/// parts of a content array joined with newlines.
fn message_text(message: &Value) -> String {
    match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                if item.get("type").and_then(Value::as_str) == Some("text") {
                    item.get("text").and_then(Value::as_str)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// A tool call's readable body: the name, then the arguments verbatim.
fn tool_call_body(name: &str, arguments: Option<&Value>) -> String {
    let input = match arguments {
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    };
    format!("{name}\n{}", crate::transcript::cap(&input, 2000))
}

/// A tool result's readable body: the `text` parts of its content joined.
fn tool_result_body(content: Option<&Value>) -> String {
    let text = match content {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                if item.get("type").and_then(Value::as_str) == Some("text") {
                    item.get("text").and_then(Value::as_str)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(Value::String(text)) => text.clone(),
        _ => String::new(),
    };
    if text.is_empty() {
        "tool result (empty)".to_owned()
    } else {
        crate::transcript::cap(&text, 4000)
    }
}

/// Write one `message` line's turns and usage. `turn` is the running ordinal
/// handed out by `begin_walk`; each written turn advances it.
fn project_line(
    store: &Store,
    session: &SessionRef,
    line: &tail::CompleteLine,
    turn: &mut u64,
    stat: &mut SyncStat,
) -> Result<()> {
    let value: Value = match serde_json::from_slice(&line.bytes) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };
    if value.get("type").and_then(Value::as_str) != Some("message") {
        return Ok(());
    }
    let Some(message) = value.get("message") else {
        return Ok(());
    };
    let ts = value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(boop_store::session::parse_iso_ms)
        .unwrap_or(0);
    let sid = session.session_id.clone();
    let role = message
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("user");
    match role {
        "user" => {
            let text = message_text(message);
            if !text.is_empty() {
                *turn += 1;
                let inserted = store.write_turn(&sid, *turn, ts, "user", &text, None)?;
                record(stat, inserted);
            }
        }
        "assistant" => {
            let mut prose = Vec::new();
            if let Some(items) = message.get("content").and_then(Value::as_array) {
                for item in items {
                    match item.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            if let Some(text) = item.get("text").and_then(Value::as_str) {
                                prose.push(text.to_owned());
                            }
                        }
                        Some("toolCall") => {
                            let name = item.get("name").and_then(Value::as_str).unwrap_or("tool");
                            *turn += 1;
                            let body = tool_call_body(name, item.get("arguments"));
                            let inserted =
                                store.write_turn(&sid, *turn, ts, "tool", &body, None)?;
                            record(stat, inserted);
                            store.write_tool_fact(&sid, *turn, ts, name, item.get("arguments"))?;
                        }
                        _ => {}
                    }
                }
            }
            let text = prose.join("\n");
            if !text.is_empty() {
                *turn += 1;
                let inserted = store.write_turn(&sid, *turn, ts, "assistant", &text, None)?;
                record(stat, inserted);
            }
            if let Some(usage) = message.get("usage") {
                let model = message
                    .get("model")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_owned();
                let message_id = format!("{sid}#t{turn}");
                let request_id = value
                    .get("responseId")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let input = usage.get("input").and_then(Value::as_i64).unwrap_or(0);
                let output = usage.get("output").and_then(Value::as_i64).unwrap_or(0);
                let cache_write = usage.get("cacheWrite").and_then(Value::as_i64).unwrap_or(0);
                let cached = usage.get("cacheRead").and_then(Value::as_i64).unwrap_or(0);
                let cost = usage
                    .get("cost")
                    .and_then(|cost| cost.get("total"))
                    .and_then(Value::as_f64);
                let usage_row = UsageRow {
                    ts,
                    message_id: &message_id,
                    request_id,
                    model: &model,
                    service_tier: None,
                    input_tokens: input,
                    output_tokens: output,
                    cache_create_5m_tokens: cache_write,
                    cache_create_1h_tokens: 0,
                    cache_read_tokens: cached,
                    is_sidechain: session.parent.is_some(),
                    cost_usd_recorded: cost,
                };
                let (is_new, changed) = store.write_usage(&sid, *turn, &usage_row)?;
                if changed {
                    if is_new {
                        stat.usage_written += 1;
                    } else {
                        stat.usage_updated += 1;
                    }
                }
            }
        }
        "toolResult" => {
            *turn += 1;
            let body = tool_result_body(message.get("content"));
            let inserted = store.write_turn(&sid, *turn, ts, "tool", &body, None)?;
            record(stat, inserted);
        }
        _ => {}
    }
    Ok(())
}

/// The strip's metadata: input tokens from the last assistant `usage`, model
/// from the last assistant message (falling back to the last `model_change`),
/// provider from the last assistant message, and the session header timestamp
/// as the created time.
fn omp_meta(path: &Path) -> (Option<u64>, Option<String>, Option<String>, u64) {
    let Ok(file) = File::open(path) else {
        return (None, None, None, 0);
    };
    let mut input = None;
    let mut model = None;
    let mut model_change = None;
    let mut provider = None;
    let mut created = 0;
    for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
        let Some(value) = crate::transcript::json(&line) else {
            continue;
        };
        match value.get("type").and_then(Value::as_str) {
            Some("session") => {
                created = value
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .and_then(boop_store::session::parse_iso_ms)
                    .unwrap_or(0);
            }
            Some("model_change") => {
                if let Some(name) = value.get("model").and_then(Value::as_str) {
                    model_change = Some(name.to_owned());
                }
            }
            Some("message") => {
                let Some(message) = value.get("message") else {
                    continue;
                };
                if message.get("role").and_then(Value::as_str) == Some("assistant") {
                    if let Some(name) = message.get("model").and_then(Value::as_str) {
                        model = Some(name.to_owned());
                    }
                    if let Some(name) = message.get("provider").and_then(Value::as_str) {
                        provider = Some(name.to_owned());
                    }
                    if let Some(usage) = message.get("usage") {
                        input = usage.get("input").and_then(Value::as_u64);
                    }
                }
            }
            _ => {}
        }
    }
    (input, model.or(model_change), provider, created)
}

/// Every `message` line as a `Message`, oldest first. `after_seq` returns only
/// newer lines.
fn read_omp(
    path: &Path,
    session_id: &str,
    after_seq: Option<u64>,
) -> Vec<crate::transcript::Message> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (seq, line) in std::io::BufReader::new(file).lines().enumerate() {
        let seq = seq as u64;
        if after_seq.is_some_and(|n| seq <= n) {
            continue;
        }
        let Ok(v) = line
            .ok()
            .and_then(|line| serde_json::from_str::<Value>(&line).ok())
            .ok_or(())
        else {
            continue;
        };
        if v.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let Some(message) = v.get("message") else {
            continue;
        };
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let text = message_text(message);
        if text.trim().is_empty() {
            continue;
        }
        let ts = v
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(boop_store::session::parse_iso_ms)
            .unwrap_or(0);
        let id = v.get("id").and_then(Value::as_str).unwrap_or("").to_owned();
        out.push(crate::transcript::Message {
            harness: HarnessId::Omp,
            session_id: session_id.to_owned(),
            id,
            seq,
            role,
            subtype: None,
            ts,
            preview: crate::transcript::cap(&text, 180),
            text,
            locator: format!("omp:{}#L{}", path.display(), seq + 1),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transcript(path: &Path, id: &str, parent: Option<&str>) {
        let mut session = serde_json::json!({
            "type": "session",
            "id": id,
            "cwd": "/fixture",
        });
        if let Some(parent) = parent {
            session["parentSession"] = serde_json::Value::String(parent.to_owned());
        }
        std::fs::write(path, format!("{session}\n")).unwrap();
    }

    #[test]
    fn omp_terminal_records_bind_only_their_exact_tmux_transcript() {
        let root = tempfile::tempdir().unwrap();
        let terminal = root.path().join("terminal-sessions");
        let sessions = root.path().join("sessions");
        std::fs::create_dir_all(&terminal).unwrap();
        std::fs::create_dir_all(&sessions).unwrap();
        let first = sessions.join("first.jsonl");
        let second = sessions.join("second.jsonl");
        transcript(&first, "first", None);
        transcript(
            &second,
            "second",
            Some("/fixture/sessions/previous-session.jsonl"),
        );
        std::fs::write(
            terminal.join("tmux-%41"),
            format!("/shared\n{}\n", first.display()),
        )
        .unwrap();
        std::fs::write(
            terminal.join("tmux-%42"),
            format!("/shared\n{}\n", second.display()),
        )
        .unwrap();
        std::fs::write(
            terminal.join("ttys999"),
            format!("/ignored\n{}\n", first.display()),
        )
        .unwrap();
        std::fs::write(terminal.join("tmux-%43"), "/stale\n/missing.jsonl\n").unwrap();

        let live = omp_live_sessions_in(&terminal).unwrap();
        assert_eq!(
            live.iter()
                .map(|session| {
                    (
                        session.tmux_pane.as_deref(),
                        session.session_id.as_str(),
                        session.parent_session.as_deref(),
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                (Some("%41"), "first", None),
                (Some("%42"), "second", None),
            ]
        );
        assert_eq!(live[0].scope, LiveSessionScope::Root);
        assert_eq!(live[1].scope, LiveSessionScope::Root);
        assert_eq!(crate::live::interactive_session_id(&live[1], &live), "second");

        std::fs::write(
            terminal.join("tmux-%41"),
            format!("/shared\n{}\n", second.display()),
        )
        .unwrap();
        assert_eq!(
            omp_live_sessions_in(&terminal)
                .unwrap()
                .into_iter()
                .find(|session| session.tmux_pane.as_deref() == Some("%41"))
                .map(|session| session.session_id),
            Some("second".into())
        );
    }
}
