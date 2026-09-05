//! The codex adapter: transcripts under `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`.
//! Every line wraps a `payload` object whose own `type` names the real record.
#![allow(dead_code)]

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use crate::harness::{
    jsonl_files, Capabilities, ControlCapabilities, Harness, HarnessId, Ingested, KnownSessions,
    LanePolicy, MailPolicy, NativeChildEvent, ReadChunk, SessionRef, SpawnSpec, VariantSupport,
};
use anyhow::Context;
use boop_store::event::AgentEvent;
use boop_store::ident::{Store, SyncStat, UsageRow};
use boop_store::tail;
use serde_json::Value;

pub struct Codex;

/// Reasoning effort rides the `model@effort` suffix, so `--variant` has no
/// spelling here; the native TUI needs the store projector beside it.
static CAPABILITIES: Capabilities = Capabilities {
    bans_plan_family_models: false,
    lanes: LanePolicy::Allowed,
    variant: VariantSupport::ModelSuffixEffort,
    mail: MailPolicy::Door,
    native_tui_projector: true,
    // false so codex renders on the primary screen: its transcript then lands in
    // tmux history, which is the only scrollback a codex pane ever gets.
    wrapper_owns_alternate_screen: false,
};

/// The state database and remote-control socket of the codex on this machine.
static DOOR: crate::door::codex::CodexDoor = crate::door::codex::CodexDoor::machine();

impl Harness for Codex {
    fn open_channel(
        &self,
        spec: &boop_acp::channel::ChannelSpec,
    ) -> anyhow::Result<Box<dyn boop_acp::channel::LaneChannel>> {
        Ok(Box::new(boop_acp::channel::acp::AcpChannel::open_adapter(
            spec,
            boop_acp::channel::acp::CODEX_ADAPTER,
        )?))
    }

    fn id(&self) -> HarnessId {
        HarnessId::Codex
    }

    fn capabilities(&self) -> &'static Capabilities {
        &CAPABILITIES
    }

    fn tui_composer(&self) -> crate::harness::TuiComposer {
        crate::harness::TuiComposer::Codex
    }

    fn live(&self) -> &dyn crate::live::LiveSessions {
        &DOOR
    }

    fn door(&self) -> &dyn crate::door::Door {
        &DOOR
    }

    /// `send_midflight` stays false: ACP takes one `session/prompt` per turn
    /// and a second before the first resolves is out of protocol.
    fn control_capabilities(&self) -> ControlCapabilities {
        ControlCapabilities {
            send_midflight: false,
            resume: true,
            spawn: true,
            subagent_visible: true,
        }
    }

    fn preview_command(&self, spec: &SpawnSpec) -> Option<String> {
        Some(crate::harness::supervisor_command(spec))
    }

    fn spawn(&self, spec: &SpawnSpec) -> anyhow::Result<SessionRef> {
        let session_id = format!("agent-{}", random_hex());
        let tmux_name = spec
            .tmux
            .clone()
            .unwrap_or_else(|| format!("boop-{session_id}"));
        let cwd = crate::worktree::prepare_spawn_dir(spec)?;
        let command = crate::harness::supervisor_command(spec);
        boop_store::tmux::mux().new_detached_session(
            spec.socket.as_deref(),
            &tmux_name,
            &cwd.display().to_string(),
            &command,
        )?;
        Ok(SessionRef {
            harness: HarnessId::Codex,
            session_id: session_id.clone(),
            nickname: session_id,
            // Codex mints its own rollout id; sync discovers the transcript
            // under the sessions dir, this handle only anchors the lane.
            path: codex_sessions_dir().unwrap_or_else(|_| cwd.join(".codex-sessions")),
            cwd: Some(cwd.display().to_string()),
            git_branch: Some(spec.branch.clone()),
            modified_ms: now_ms(),
            size: 0,
            tmux: Some(tmux_name),
            tmux_socket: spec.socket.clone(),
            parent: None,
        })
    }

    fn stop(&self, session: &SessionRef) -> anyhow::Result<()> {
        if let Some(tmux) = &session.tmux {
            if boop_store::tmux::mux().has_session(session.tmux_socket.as_deref(), tmux)? {
                boop_store::tmux::mux().kill_session(session.tmux_socket.as_deref(), tmux)?;
            }
        }
        Ok(())
    }

    fn sessions(&self) -> anyhow::Result<Vec<SessionRef>> {
        sessions_in(&codex_sessions_dir()?)
    }

    fn session_roots(&self) -> anyhow::Result<Vec<PathBuf>> {
        Ok(vec![codex_sessions_dir()?])
    }

    fn sync_candidates(&self, known: &KnownSessions) -> anyhow::Result<Vec<SessionRef>> {
        sessions_in_with_known(&codex_sessions_dir()?, known)
    }

    fn read_from(&self, session: &SessionRef, offset: u64) -> anyhow::Result<ReadChunk> {
        let mut file = File::open(&session.path)
            .with_context(|| format!("open transcript {}", session.path.display()))?;
        let result = tail::read_complete_lines(&mut file, offset)?;

        let mut events = Vec::new();
        let mut skipped = 0usize;
        for line in &result.lines {
            match parse_line(session, line) {
                Some(event) => events.push(event),
                None => skipped += 1,
            }
        }

        Ok(ReadChunk {
            events,
            next_offset: result.next_offset,
            reset: result.reset,
            skipped,
        })
    }

    /// `token_count` snapshots carry no per-call id; a turn's snapshots sum
    /// into one row keyed `{session}#t{turn}` (dict_request is global).
    fn ingest(&self, store: &Store, session: &SessionRef, from: u64) -> anyhow::Result<Ingested> {
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
        let mut current_model = String::from("unknown");
        let mut turn_tokens = TurnTokens::default();
        for line in &result.lines {
            project_line(
                store,
                session,
                line,
                &mut turn,
                &mut stat,
                &mut current_model,
                &mut turn_tokens,
            )?;
        }
        Ok(Ingested {
            stat,
            next_cursor: result.next_offset,
        })
    }

    fn observe_native_children(
        &self,
        session: &SessionRef,
        from: u64,
    ) -> anyhow::Result<Vec<NativeChildEvent>> {
        let Some(parent_session) = session.parent.as_deref() else {
            return Ok(Vec::new());
        };
        // Approval reviewers also carry parent_thread_id. Their task_complete
        // records must not wake that parent as if delegated work had finished.
        // Read the header independently of `from`: incremental tails usually
        // start after session_meta, and the cached SessionRef keeps ancestry.
        if first_session_meta(&session.path).is_some_and(|meta| meta.is_guardian) {
            return Ok(Vec::new());
        }
        let mut file = File::open(&session.path)
            .with_context(|| format!("open transcript {}", session.path.display()))?;
        let result = tail::read_complete_lines(&mut file, from)?;
        Ok(native_child_events_from_lines(
            parent_session,
            &session.session_id,
            &result.lines,
        ))
    }

    fn native_child_completion_visible(
        &self,
        parent_session: &str,
        child_session: &str,
    ) -> anyhow::Result<bool> {
        let Some(parent) = self
            .sessions()?
            .into_iter()
            .find(|session| session.session_id == parent_session)
        else {
            return Ok(false);
        };
        let mut file = File::open(&parent.path)
            .with_context(|| format!("open parent transcript {}", parent.path.display()))?;
        let lines = tail::read_complete_lines(&mut file, 0)?.lines;
        Ok(native_completion_satisfies_delivery(&lines, child_session))
    }
}

fn codex_sessions_dir() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().context("resolve home directory")?;
    Ok(home.join(".codex").join("sessions"))
}

fn random_hex() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mixed = (nanos as u64) ^ ((std::process::id() as u64) << 48) ^ (nanos >> 64) as u64;
    format!("{mixed:016x}")
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A file whose first line is not `session_meta` is skipped, never guessed
/// at from the filename.
struct SessionMeta {
    session_id: String,
    parent: Option<String>,
    cwd: Option<String>,
    nickname: String,
    is_guardian: bool,
}

fn first_session_meta(path: &Path) -> Option<SessionMeta> {
    let mut reader = BufReader::new(File::open(path).ok()?);
    let first = tail::read_first_complete_line(&mut reader).ok()??;
    let value: Value = serde_json::from_slice(&first.bytes).ok()?;
    if value.get("type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }
    let payload = value.get("payload")?.as_object()?;
    let session_id = payload.get("id").and_then(Value::as_str)?.to_owned();
    let parent = payload
        .get("parent_thread_id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let parent = parent.or_else(|| {
        payload
            .get("forked_from_id")
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    let cwd = payload
        .get("cwd")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let nickname = payload
        .get("agent_nickname")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| session_id.clone());
    Some(SessionMeta {
        session_id,
        parent,
        cwd,
        nickname,
        is_guardian: value
            .pointer("/payload/source/subagent/other")
            .and_then(Value::as_str)
            == Some("guardian"),
    })
}

/// Codex records subagent parentage in the child's `session_meta` and writes
/// `task_complete` in that child's transcript. These are durable local events;
/// app-server remote control is used only later, by the shared projector, to
/// notify a registered parent route.
fn native_child_events_from_lines(
    parent_session: &str,
    child_session: &str,
    lines: &[tail::CompleteLine],
) -> Vec<NativeChildEvent> {
    let mut events = Vec::new();
    for line in lines {
        let Ok(value) = serde_json::from_slice::<Value>(&line.bytes) else {
            continue;
        };
        let at_ms = value
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(crate::harness::claude::parse_iso_ms)
            .unwrap_or(0);
        let outer = value.get("type").and_then(Value::as_str);
        let record = value
            .get("payload")
            .and_then(Value::as_object)
            .and_then(|payload| payload.get("type"))
            .and_then(Value::as_str);
        if outer == Some("session_meta") {
            events.push(NativeChildEvent::Spawned {
                parent_session: parent_session.to_owned(),
                child_session: child_session.to_owned(),
                at_ms,
            });
        }
        if outer == Some("event_msg") && record == Some("task_complete") {
            events.push(NativeChildEvent::Completed {
                parent_session: parent_session.to_owned(),
                child_session: child_session.to_owned(),
                outcome: "completed".to_owned(),
                at_ms,
            });
        }
    }
    events
}

fn native_completion_notification(line: &[u8], child_session: &str) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(line) else {
        return false;
    };
    if value.get("type").and_then(Value::as_str) != Some("response_item")
        || value.pointer("/payload/type").and_then(Value::as_str) != Some("message")
        || value.pointer("/payload/role").and_then(Value::as_str) != Some("user")
    {
        return false;
    }
    value
        .pointer("/payload/content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .filter_map(|text| {
            text.strip_prefix("<subagent_notification>\n")?
                .strip_suffix("\n</subagent_notification>")
        })
        .filter_map(|body| serde_json::from_str::<Value>(body).ok())
        .any(|notification| {
            notification.get("agent_path").and_then(Value::as_str) == Some(child_session)
                && notification.pointer("/status/completed").is_some()
        })
}

fn native_completion_satisfies_delivery(lines: &[tail::CompleteLine], child_session: &str) -> bool {
    parent_turn_is_active(lines)
        && lines
            .iter()
            .any(|line| native_completion_notification(&line.bytes, child_session))
}

fn parent_turn_is_active(lines: &[tail::CompleteLine]) -> bool {
    lines.iter().fold(false, |active, line| {
        let Ok(value) = serde_json::from_slice::<Value>(&line.bytes) else {
            return active;
        };
        if value.get("type").and_then(Value::as_str) != Some("event_msg") {
            return active;
        }
        match value.pointer("/payload/type").and_then(Value::as_str) {
            Some("task_started") => true,
            Some("task_complete") => false,
            _ => active,
        }
    })
}

fn sessions_in(base: &Path) -> anyhow::Result<Vec<SessionRef>> {
    sessions_in_with_known(base, &KnownSessions::new())
}

fn sessions_in_with_known(base: &Path, known: &KnownSessions) -> anyhow::Result<Vec<SessionRef>> {
    let mut sessions = Vec::new();
    for file in jsonl_files(base)? {
        let path = &file.path;
        let modified_ms = file.modified_ms;
        let size = file.size;

        if let Some(known) = known.get(path) {
            sessions.push(SessionRef {
                harness: HarnessId::Codex,
                session_id: known.session_id.clone(),
                nickname: known.nickname.clone(),
                path: path.to_path_buf(),
                cwd: known.cwd.clone(),
                git_branch: known.git_branch.clone(),
                modified_ms,
                size,
                tmux: None,
                tmux_socket: None,
                parent: known.parent.clone(),
            });
            continue;
        }

        let Some(meta) = first_session_meta(path) else {
            continue;
        };

        sessions.push(SessionRef {
            harness: HarnessId::Codex,
            session_id: meta.session_id,
            nickname: meta.nickname,
            path: path.to_path_buf(),
            cwd: meta.cwd,
            git_branch: None,
            modified_ms,
            size,
            tmux: None,
            tmux_socket: None,
            parent: meta.parent,
        });
    }
    sessions.sort_by_key(|session| session.modified_ms);
    Ok(sessions)
}

/// The newest root Codex transcript whose recorded cwd is exactly `cwd`.
/// Native subagent transcripts are excluded because their parent is present.
pub fn latest_root_session_for_cwd(cwd: &Path) -> anyhow::Result<Option<SessionRef>> {
    let cwd = cwd.display().to_string();
    Ok(sessions_in(&codex_sessions_dir()?)?
        .into_iter()
        .rev()
        .find(|session| session.cwd.as_deref() == Some(cwd.as_str()) && session.parent.is_none()))
}

/// `record_type` is `payload.type`, since the outer wrapper only ever says
/// `session_meta`/`response_item`/`event_msg`.
fn parse_line(session: &SessionRef, line: &tail::CompleteLine) -> Option<AgentEvent> {
    let value: Value = serde_json::from_slice(&line.bytes).ok()?;
    let ts_ms = value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(crate::harness::claude::parse_iso_ms)
        .unwrap_or(0);
    let outer_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let payload = value.get("payload").and_then(Value::as_object);
    let record_type = payload
        .and_then(|payload| payload.get("type"))
        .and_then(Value::as_str)
        .unwrap_or(outer_type)
        .to_owned();
    let uuid = payload
        .and_then(|payload| payload.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let record_cwd = payload
        .and_then(|payload| payload.get("cwd"))
        .and_then(Value::as_str)
        .map(str::to_owned);

    let mut tool_name = None;
    let mut paths = Vec::new();
    if let Some(payload) = payload {
        match record_type.as_str() {
            "function_call" | "custom_tool_call" => {
                tool_name = payload
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
            "patch_apply_end" => {
                if let Some(changes) = payload.get("changes").and_then(Value::as_object) {
                    for (path, change) in changes {
                        paths.push(boop_store::event::ToolPath {
                            path: path.clone(),
                            access: access_for_change(change),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    Some(AgentEvent {
        harness: session.harness.as_str(),
        session_id: session.session_id.clone(),
        ts_ms,
        uuid,
        parent_uuid: None,
        cwd: record_cwd.or_else(|| session.cwd.clone()),
        git_branch: session.git_branch.clone(),
        record_type,
        tool_name,
        paths,
        urls: Vec::new(),
        raw_line_offset: line.start,
    })
}

fn access_for_change(change: &Value) -> boop_store::event::Access {
    match change.get("type").and_then(Value::as_str) {
        Some("add") => boop_store::event::Access::Create,
        Some("delete") => boop_store::event::Access::Delete,
        _ => boop_store::event::Access::Write,
    }
}

fn record(stat: &mut SyncStat, inserted: usize) {
    if inserted == 0 {
        stat.dropped += 1;
    } else {
        stat.written += 1;
    }
}

/// `reasoning`/other block kinds are skipped: a thinking-only block burns no
/// ordinal, mirroring claude.
fn message_text(payload: &serde_json::Map<String, Value>) -> String {
    let Some(blocks) = payload.get("content").and_then(Value::as_array) else {
        return String::new();
    };
    let mut parts = Vec::new();
    for block in blocks {
        let Some(block) = block.as_object() else {
            continue;
        };
        let kind = block.get("type").and_then(Value::as_str).unwrap_or("");
        if kind == "input_text" || kind == "output_text" || kind == "text" {
            if let Some(text) = block.get("text").and_then(Value::as_str) {
                parts.push(text.to_owned());
            }
        }
    }
    parts.join("\n")
}

/// A char-boundary-safe prefix; byte slicing risks panicking mid multi-byte
/// character.
fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

/// The string itself, or its JSON form when it is not a string.
fn value_as_text(value: &Value) -> String {
    match value.as_str() {
        Some(text) => text.to_owned(),
        None => value.to_string(),
    }
}

/// A tool call's readable body: the name, then the input verbatim. Input
/// rides `input` (custom tools) or `arguments` (function calls).
fn tool_call_body(name: &str, payload: &serde_json::Map<String, Value>) -> String {
    let input = payload
        .get("input")
        .or_else(|| payload.get("arguments"))
        .map(value_as_text)
        .unwrap_or_default();
    format!("{name}\n{}", truncate_chars(&input, 2000))
}

/// `output`'s `input_text` parts joined, or the string verbatim.
/// An unrecognized shape falls back to the whole payload, never empty.
fn tool_output_body(payload: &serde_json::Map<String, Value>) -> String {
    let text = match payload.get("output") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => other.to_string(),
        None => String::new(),
    };
    let text = match text.is_empty() {
        true => serde_json::to_string(payload).unwrap_or_default(),
        false => text,
    };
    truncate_chars(&text, 4000)
}

/// A patch's call id and the paths it touched; there is no prose beyond the
/// file list.
fn patch_body(call_id: &str, files: &[String]) -> String {
    match files.is_empty() {
        true => format!("patch {call_id}"),
        false => format!("patch {call_id}\nfiles: {}", files.join(", ")),
    }
}

#[allow(clippy::too_many_arguments)]
/// Codex record types that carry session bookkeeping and no transcript
/// content. `user_message` duplicates the `response_item` message that
/// precedes it; `task_started`/`task_complete` bracket a turn the message rows
/// already delimit; the rest are session and context metadata.
const BOOKKEEPING: &[&str] = &[
    "session_meta",
    "turn_context",
    "world_state",
    "task_started",
    "task_complete",
    "user_message",
    "context_compacted",
    "compacted",
    "item_started",
    "item_completed",
    "sub_agent_activity",
    "inter_agent_communication_metadata",
    "turn_aborted",
    "thread_rolled_back",
    "tool_search_call",
    "tool_search_output",
];

/// Running totals for one turn's `token_count` snapshots.
#[derive(Default)]
struct TurnTokens {
    turn: u64,
    input: i64,
    output: i64,
    cache_write: i64,
    cached: i64,
}

fn project_line(
    store: &Store,
    session: &SessionRef,
    line: &tail::CompleteLine,
    turn: &mut u64,
    stat: &mut SyncStat,
    current_model: &mut String,
    turn_tokens: &mut TurnTokens,
) -> anyhow::Result<()> {
    let value: Value = match serde_json::from_slice(&line.bytes) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };
    let ts = value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(crate::harness::claude::parse_iso_ms)
        .unwrap_or(0);
    let Some(payload) = value.get("payload").and_then(Value::as_object) else {
        return Ok(());
    };
    let record_type = payload.get("type").and_then(Value::as_str).unwrap_or("");
    let outer_type = value.get("type").and_then(Value::as_str).unwrap_or("");
    let sid = session.session_id.clone();

    match record_type {
        "message" => {
            let role = payload
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("user");
            let text = message_text(payload);
            if !text.is_empty() {
                *turn += 1;
                let inserted = store.write_turn(&sid, *turn, ts, role, &text)?;
                record(stat, inserted);
            }
        }
        "function_call" | "custom_tool_call" => {
            // Argument shapes vary by tool/version; no agent_cmd/agent_touch
            // fact is derived here.
            let name = payload
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("tool");
            *turn += 1;
            let body = tool_call_body(name, payload);
            let inserted = store.write_turn(&sid, *turn, ts, "tool", &body)?;
            record(stat, inserted);
            store.write_tool_fact(&sid, *turn, ts, name, None)?;
        }
        "function_call_output" | "custom_tool_call_output" => {
            *turn += 1;
            let body = tool_output_body(payload);
            let inserted = store.write_turn(&sid, *turn, ts, "tool", &body)?;
            record(stat, inserted);
        }
        "agent_message" => {
            let text = payload
                .get("message")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| serde_json::to_string(payload).unwrap_or_default());
            *turn += 1;
            let inserted = store.write_turn(&sid, *turn, ts, "assistant", &text)?;
            record(stat, inserted);
        }
        "patch_apply_end" => {
            let Some(changes) = payload.get("changes").and_then(Value::as_object) else {
                return Ok(());
            };
            if changes.is_empty() {
                return Ok(());
            }
            let call_id = payload
                .get("call_id")
                .and_then(Value::as_str)
                .unwrap_or("patch");
            let files: Vec<String> = changes.keys().cloned().collect();
            *turn += 1;
            let body = patch_body(call_id, &files);
            let inserted = store.write_turn(&sid, *turn, ts, "tool", &body)?;
            record(stat, inserted);
            for (path, change) in changes {
                // The store's verb vocabulary (read/write/edit/...) has no
                // delete; a deleted file is not recorded as a touch.
                let verb = match change.get("type").and_then(Value::as_str) {
                    Some("add") => "Write",
                    Some("update") => "Edit",
                    _ => continue,
                };
                store.write_tool_fact(
                    &sid,
                    *turn,
                    ts,
                    verb,
                    Some(&serde_json::json!({ "file_path": path })),
                )?;
            }
        }
        "thread_settings_applied" => {
            if let Some(model) = payload
                .get("thread_settings")
                .and_then(|settings| settings.get("model"))
                .and_then(Value::as_str)
            {
                *current_model = model.to_owned();
            }
        }
        "token_count" => {
            let Some(last) = payload
                .get("info")
                .and_then(|info| info.get("last_token_usage"))
                .and_then(Value::as_object)
            else {
                return Ok(());
            };
            let count = |key: &str| -> i64 { last.get(key).and_then(Value::as_i64).unwrap_or(0) };
            // `input_tokens` includes the cached subset (OTEL convention); the
            // store wants it excluded, so it is subtracted out here.
            let cached = count("cached_input_tokens");
            let cache_write = count("cache_write_input_tokens");
            let input_tokens = (count("input_tokens") - cached - cache_write).max(0);
            let attach_turn = if *turn == 0 {
                *turn += 1;
                let inserted = store.write_turn(&sid, *turn, ts, "assistant", "")?;
                record(stat, inserted);
                *turn
            } else {
                *turn
            };
            if turn_tokens.turn != attach_turn {
                *turn_tokens = TurnTokens {
                    turn: attach_turn,
                    ..TurnTokens::default()
                };
            }
            turn_tokens.input += input_tokens;
            turn_tokens.output += count("output_tokens");
            turn_tokens.cache_write += cache_write;
            turn_tokens.cached += cached;
            let message_id = format!("{sid}#t{attach_turn}");
            let usage = UsageRow {
                ts,
                message_id: &message_id,
                request_id: "",
                model: current_model.as_str(),
                service_tier: None,
                input_tokens: turn_tokens.input,
                output_tokens: turn_tokens.output,
                cache_create_5m_tokens: turn_tokens.cache_write,
                cache_create_1h_tokens: 0,
                cache_read_tokens: turn_tokens.cached,
                is_sidechain: session.parent.is_some(),
                cost_usd_recorded: None,
            };
            let (is_new, changed) = store.write_usage(&sid, attach_turn, &usage)?;
            if changed {
                if is_new {
                    stat.usage_written += 1;
                } else {
                    stat.usage_updated += 1;
                }
            }
        }
        "reasoning" => {
            // Codex reasoning carries only its summary text. Empty summaries
            // (the common case) leave no row.
            let text = payload
                .get("summary")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.get("text").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default();
            if !text.is_empty() {
                *turn += 1;
                let body = format!("(reasoning)\n{text}");
                let inserted = store.write_turn(&sid, *turn, ts, "assistant", &body)?;
                record(stat, inserted);
            }
        }
        "agent_reasoning" => {
            // The event_msg twin of `reasoning`: the summary is already one
            // assembled string rather than a list of parts.
            let text = payload.get("text").and_then(Value::as_str).unwrap_or("");
            if !text.is_empty() {
                *turn += 1;
                let body = format!("(reasoning)\n{}", truncate_chars(text, 4000));
                let inserted = store.write_turn(&sid, *turn, ts, "assistant", &body)?;
                record(stat, inserted);
            }
        }
        "web_search_call" | "web_search_end" => {
            // The query rides `query` on the event and `action.query` on the
            // response item. The search fact comes off `web_search_end` alone:
            // a session carrying both records would otherwise count each
            // search twice.
            let query = payload
                .get("query")
                .and_then(Value::as_str)
                .or_else(|| {
                    payload
                        .get("action")
                        .and_then(|action| action.get("query"))
                        .and_then(Value::as_str)
                })
                .unwrap_or("");
            if query.is_empty() {
                return Ok(());
            }
            *turn += 1;
            let body = match payload.get("results").and_then(Value::as_array) {
                Some(results) => format!("web_search {query}\nresults: {}", results.len()),
                None => format!("web_search {query}"),
            };
            let inserted = store.write_turn(&sid, *turn, ts, "tool", &body)?;
            record(stat, inserted);
            if record_type == "web_search_end" {
                store.write_tool_fact(
                    &sid,
                    *turn,
                    ts,
                    "WebSearch",
                    Some(&serde_json::json!({ "query": query })),
                )?;
            }
        }
        "mcp_tool_call_end" => {
            let invocation = payload.get("invocation");
            let field = |key: &str| -> &str {
                invocation
                    .and_then(|invocation| invocation.get(key))
                    .and_then(Value::as_str)
                    .unwrap_or("")
            };
            let server = field("server");
            let tool = field("tool");
            let name = match (server.is_empty(), tool.is_empty()) {
                (true, true) => "mcp".to_owned(),
                (true, false) => tool.to_owned(),
                (false, true) => server.to_owned(),
                (false, false) => format!("{server}__{tool}"),
            };
            let arguments = invocation
                .and_then(|invocation| invocation.get("arguments"))
                .map(value_as_text)
                .unwrap_or_default();
            *turn += 1;
            let body = format!("{name}\n{}", truncate_chars(&arguments, 2000));
            let inserted = store.write_turn(&sid, *turn, ts, "tool", &body)?;
            record(stat, inserted);
            store.write_tool_fact(&sid, *turn, ts, &name, None)?;
        }
        kind if BOOKKEEPING.contains(&if kind.is_empty() { outer_type } else { kind }) => {
            // Session bookkeeping, no transcript content: nothing to project
            // and nothing to warn about. WARN here printed into the codex
            // pane on every `boop` call the agent made.
            tracing::debug!(
                record = if kind.is_empty() { outer_type } else { kind },
                session_id = %sid,
                "codex bookkeeping record skipped"
            );
        }
        kind => {
            let label = if kind.is_empty() { outer_type } else { kind };
            *turn += 1;
            let raw = truncate_chars(&value.to_string(), 4000);
            let body = format!("{label} (unprojected)\n{raw}");
            let inserted = store.write_turn(&sid, *turn, ts, "tool", &body)?;
            record(stat, inserted);
            // One line per kind per process. Before this, a single session's
            // 9734 `sub_agent_activity` records printed 9734 WARN lines into
            // the pane codex draws its TUI in.
            if crate::harness::first_projection_gap("codex", label) {
                tracing::warn!(
                    projection_gap = label,
                    session_id = %sid,
                    turn = *turn,
                    "codex record type projected as raw json"
                );
            } else {
                tracing::debug!(
                    projection_gap = label,
                    session_id = %sid,
                    turn = *turn,
                    "codex record type projected as raw json"
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::harness::HarnessId;
    use std::path::PathBuf;

    use std::fs::OpenOptions;
    use std::io::Write;

    use crate::harness::{claude, sync_session, Harness, KnownSession, KnownSessions, SessionRef};
    use boop_store::ident::TurnQuery;
    use boop_store::testing::TempRepo;
    use boop_store::Store;

    use super::{
        native_child_events_from_lines, native_completion_notification,
        native_completion_satisfies_delivery, sessions_in, sessions_in_with_known, Codex,
    };

    #[test]
    fn native_completion_requires_the_structured_notification_and_exact_child() {
        let exact = br#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<subagent_notification>\n{\"agent_path\":\"child-session\",\"status\":{\"completed\":\"done\"}}\n</subagent_notification>"}]}}"#;
        let wrong_child = br#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<subagent_notification>\n{\"agent_path\":\"other-child\",\"status\":{\"completed\":\"done\"}}\n</subagent_notification>"}]}}"#;
        let ordinary_text = br#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"child-session completed"}]}}"#;

        assert!(native_completion_notification(exact, "child-session"));
        assert!(!native_completion_notification(
            wrong_child,
            "child-session"
        ));
        assert!(!native_completion_notification(
            ordinary_text,
            "child-session"
        ));
    }

    #[test]
    fn idle_native_notification_keeps_the_queue_injection_fallback() {
        let lines = [boop_store::tail::CompleteLine {
            start: 0,
            bytes: br#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<subagent_notification>\n{\"agent_path\":\"child-session\",\"status\":{\"completed\":\"done\"}}\n</subagent_notification>"}]}}"#.to_vec(),
        }];

        assert!(!native_completion_satisfies_delivery(
            &lines,
            "child-session"
        ));
    }

    #[test]
    fn active_native_notification_satisfies_delivery_without_queue_injection() {
        let lines = [
            boop_store::tail::CompleteLine {
                start: 0,
                bytes: br#"{"type":"event_msg","payload":{"type":"task_started"}}"#.to_vec(),
            },
            boop_store::tail::CompleteLine {
                start: 1,
                bytes: br#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<subagent_notification>\n{\"agent_path\":\"child-session\",\"status\":{\"completed\":\"done\"}}\n</subagent_notification>"}]}}"#.to_vec(),
            },
        ];

        assert!(native_completion_satisfies_delivery(
            &lines,
            "child-session"
        ));
    }

    #[test]
    fn completed_parent_turn_keeps_the_queue_injection_fallback() {
        let lines = [
            boop_store::tail::CompleteLine {
                start: 0,
                bytes: br#"{"type":"event_msg","payload":{"type":"task_started"}}"#.to_vec(),
            },
            boop_store::tail::CompleteLine {
                start: 1,
                bytes: br#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<subagent_notification>\n{\"agent_path\":\"child-session\",\"status\":{\"completed\":\"done\"}}\n</subagent_notification>"}]}}"#.to_vec(),
            },
            boop_store::tail::CompleteLine {
                start: 2,
                bytes: br#"{"type":"event_msg","payload":{"type":"task_complete"}}"#.to_vec(),
            },
        ];

        assert!(!native_completion_satisfies_delivery(
            &lines,
            "child-session"
        ));
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("boop_codex_{}_{}", std::process::id(), name))
    }

    fn write_lines(path: &PathBuf, lines: &[&str]) {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)
            .unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
    }

    fn session_for(path: &std::path::Path, size: u64) -> SessionRef {
        SessionRef {
            harness: HarnessId::Codex,
            session_id: "ses-codex-1".to_owned(),
            nickname: "ses-codex-1".to_owned(),
            path: path.to_path_buf(),
            cwd: None,
            git_branch: None,
            modified_ms: 0,
            size,
            tmux: None,
            tmux_socket: None,
            parent: None,
        }
    }

    /// Ingest one raw jsonl line and return the turns it projects.
    fn project_one_line(name: &str, line: &str) -> Vec<serde_json::Value> {
        let base = temp_path(&format!("project-{name}"));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let transcript = base.join("session.jsonl");
        write_lines(&transcript, &[line]);
        let store_path = base.join("store.db");
        let store = Store::open(store_path).unwrap();
        let size = std::fs::metadata(&transcript).unwrap().len();
        let session = session_for(&transcript, size);
        sync_session(&store, &Codex, &session).unwrap();
        let turns = store
            .query_turns(&TurnQuery {
                session: Some(session.session_id.clone()),
                ..TurnQuery::default()
            })
            .unwrap();
        drop(store);
        std::fs::remove_dir_all(base).unwrap();
        turns
    }

    #[test]
    fn codex_capabilities_are_measured() {
        let caps = Codex.control_capabilities();
        assert!(!caps.send_midflight, "acp takes one prompt per turn");
        assert!(caps.resume);
        assert!(caps.spawn);
        assert!(caps.subagent_visible);
    }

    fn spawn_spec(socket: Option<String>) -> crate::harness::SpawnSpec {
        crate::harness::SpawnSpec {
            effort: None,
            harness: HarnessId::Codex,
            branch: "lane-test".to_owned(),
            base_sha: "0000000000000000000000000000000000000000".to_owned(),
            main_tree: true,
            setup: Vec::new(),
            prompt: "do the lane".to_owned(),
            resume_session: None,
            socket,
            worktree_dir: None,
            repo: std::env::temp_dir(),
            env_stamp: None,
            model: None,
            variant: None,
            bin: None,
            on_exit: None,
            tmux: None,
            lane: "lane-test".to_owned(),
            mail_dir: std::env::temp_dir(),
            warm_start: false,
        }
    }

    struct TmuxGuard {
        socket: String,
    }

    static NEXT_SOCKET: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    impl TmuxGuard {
        /// One socket per guard: tests run in parallel and a shared name makes
        /// each new guard kill its neighbour's server.
        fn new() -> TmuxGuard {
            let socket = format!(
                "boop-test-{}-cdx{}",
                std::process::id(),
                NEXT_SOCKET.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            );
            boop_store::tmux::kill_test_server(&socket);
            TmuxGuard { socket }
        }
    }

    impl Drop for TmuxGuard {
        fn drop(&mut self) {
            boop_store::tmux::kill_test_server(&self.socket);
        }
    }

    fn has_session_on(guard: &TmuxGuard, name: &str) -> bool {
        boop_store::tmux::mux()
            .has_session(Some(&guard.socket), name)
            .unwrap_or(false)
    }

    #[test]
    fn codex_spawn_returns_handle_and_stop_tears_down() {
        let guard = TmuxGuard::new();
        let repo = TempRepo::new();
        let mut req = spawn_spec(Some(guard.socket.clone()));
        req.main_tree = false;
        req.base_sha = repo.sha.clone();
        req.repo = repo.dir.clone();
        req.worktree_dir = Some(repo.worktree.clone());
        req.model = Some("gpt-5.6-luna@medium".to_owned());
        let codex = Codex;
        let session = codex.spawn(&req).unwrap();
        assert!(
            repo.worktree.join("seed.txt").exists(),
            "worktree must be created by spawn"
        );
        assert_eq!(
            session
                .tmux
                .as_deref()
                .map(|t| t.starts_with("boop-agent-")),
            Some(true)
        );
        assert_eq!(session.tmux_socket.as_deref(), Some(guard.socket.as_str()));
        assert!(has_session_on(&guard, session.tmux.as_deref().unwrap()));
        codex.stop(&session).unwrap();
        assert!(!has_session_on(&guard, session.tmux.as_deref().unwrap()));
    }

    #[test]
    fn reads_a_message_and_a_token_count_line() {
        let path = temp_path("jn1");
        write_lines(
            &path,
            &[
                r#"{"timestamp":"2026-08-09T17:20:05.152Z","type":"response_item","payload":{"type":"message","id":"msg_1","role":"user","content":[{"type":"input_text","text":"hello"}]}}"#,
                r#"{"timestamp":"2026-08-09T17:20:06.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":100,"cached_input_tokens":40,"output_tokens":10}}}}"#,
            ],
        );
        let metadata = path.metadata().unwrap();
        let codex = Codex;
        let chunk = codex
            .read_from(&session_for(&path, metadata.len()), 0)
            .unwrap();
        assert_eq!(chunk.events.len(), 2);
        assert_eq!(chunk.skipped, 0);
        assert_eq!(chunk.events[0].record_type, "message");
        assert_eq!(chunk.events[1].record_type, "token_count");
    }

    #[test]
    fn native_child_observer_reads_session_parent_and_completion_records() {
        let lines = [
            boop_store::tail::CompleteLine {
                start: 0,
                bytes: br#"{"timestamp":"2026-08-09T17:20:05.152Z","type":"session_meta","payload":{"id":"child-session","parent_thread_id":"parent-session"}}"#.to_vec(),
            },
            boop_store::tail::CompleteLine {
                start: 1,
                bytes: br#"{"timestamp":"2026-08-09T17:20:06.000Z","type":"event_msg","payload":{"type":"task_complete"}}"#.to_vec(),
            },
        ];
        assert_eq!(
            native_child_events_from_lines("parent-session", "child-session", &lines),
            [
                crate::harness::NativeChildEvent::Spawned {
                    parent_session: "parent-session".into(),
                    child_session: "child-session".into(),
                    at_ms: 1_786_296_005_152,
                },
                crate::harness::NativeChildEvent::Completed {
                    parent_session: "parent-session".into(),
                    child_session: "child-session".into(),
                    outcome: "completed".into(),
                    at_ms: 1_786_296_006_000,
                },
            ]
        );
    }

    #[test]
    fn parent_thread_id_wins_and_forked_from_id_remains_a_legacy_fallback() {
        let preferred = temp_path("parent-precedence");
        write_lines(
            &preferred,
            &[
                r#"{"type":"session_meta","payload":{"id":"child","parent_thread_id":"thread-parent","forked_from_id":"fork-parent"}}"#,
            ],
        );
        assert_eq!(
            super::first_session_meta(&preferred)
                .and_then(|meta| meta.parent)
                .as_deref(),
            Some("thread-parent")
        );

        let legacy = temp_path("parent-legacy");
        write_lines(
            &legacy,
            &[r#"{"type":"session_meta","payload":{"id":"child","forked_from_id":"fork-parent"}}"#],
        );
        assert_eq!(
            super::first_session_meta(&legacy)
                .and_then(|meta| meta.parent)
                .as_deref(),
            Some("fork-parent")
        );
    }

    #[test]
    fn native_child_observer_projects_the_codex_child_fixture() {
        let base = std::path::PathBuf::from("tests/fixtures/codex");
        let child = sessions_in(&base)
            .unwrap()
            .into_iter()
            .find(|session| session.parent.is_some())
            .expect("Codex child fixture");
        let events = Codex.observe_native_children(&child, 0).unwrap();
        assert_eq!(
            events,
            [
                crate::harness::NativeChildEvent::Spawned {
                    parent_session: "00000000-0000-7000-8000-000000000001".into(),
                    child_session: "00000000-0000-7000-8000-000000000002".into(),
                    at_ms: 1_786_284_300_000,
                },
                crate::harness::NativeChildEvent::Completed {
                    parent_session: "00000000-0000-7000-8000-000000000001".into(),
                    child_session: "00000000-0000-7000-8000-000000000002".into(),
                    outcome: "completed".into(),
                    at_ms: 1_786_284_312_000,
                },
            ]
        );
    }

    /// Fail-first receipt: pre-fix, same-turn `token_count` snapshots each
    /// INSERTed and collided on the agent_usage (session_id, turn) key.
    #[test]
    fn same_turn_token_counts_sum_into_one_usage_row() {
        let db_path = temp_path("jn4db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let path = temp_path("jn4");
        write_lines(
            &path,
            &[
                r#"{"timestamp":"2026-08-09T17:20:05.152Z","type":"response_item","payload":{"type":"message","id":"msg_1","role":"user","content":[{"type":"input_text","text":"hello"}]}}"#,
                r#"{"timestamp":"2026-08-09T17:20:06.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":100,"cached_input_tokens":40,"output_tokens":10}}}}"#,
                r#"{"timestamp":"2026-08-09T17:20:07.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":50,"cached_input_tokens":0,"output_tokens":5}}}}"#,
                r#"{"timestamp":"2026-08-09T17:20:08.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":10,"cached_input_tokens":10,"output_tokens":1}}}}"#,
            ],
        );
        let metadata = path.metadata().unwrap();
        let codex = Codex;
        let ingested = codex
            .ingest(&store, &session_for(&path, metadata.len()), 0)
            .unwrap();
        assert_eq!(ingested.stat.usage_written, 1);
        assert_eq!(ingested.stat.usage_updated, 2);
        drop(store);
        let totals = boop_store::testing::usage_totals_at(&db_path);
        let (row_count, input_tokens, output_tokens, cache_read_tokens) = (
            totals.row_count,
            totals.input_tokens,
            totals.output_tokens,
            totals.cache_read_tokens,
        );
        assert_eq!(row_count, 1);
        assert_eq!(input_tokens, 110);
        assert_eq!(output_tokens, 16);
        assert_eq!(cache_read_tokens, 50);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn skips_an_invalid_json_line_but_keeps_the_rest() {
        let path = temp_path("jn2");
        write_lines(
            &path,
            &[
                r#"{"timestamp":"2026-08-09T17:20:05.152Z","type":"response_item","payload":{"type":"message","id":"msg_1","role":"user","content":[{"type":"input_text","text":"hi"}]}}"#,
                r#"this is not json at all"#,
            ],
        );
        let metadata = path.metadata().unwrap();
        let codex = Codex;
        let chunk = codex
            .read_from(&session_for(&path, metadata.len()), 0)
            .unwrap();
        assert_eq!(chunk.events.len(), 1);
        assert_eq!(chunk.skipped, 1);
    }

    /// A partial trailing line is neither parsed nor counted into the offset;
    /// the harness trait doc requires this for every file-backed adapter.
    #[test]
    fn a_partial_trailing_line_is_not_consumed() {
        let path = temp_path("jn3");
        write_lines(
            &path,
            &[
                r#"{"timestamp":"2026-08-09T17:20:05.152Z","type":"response_item","payload":{"type":"message","id":"msg_1","role":"user","content":[{"type":"input_text","text":"hi"}]}}"#,
            ],
        );
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        write!(file, "{{\"partial").unwrap();
        drop(file);
        let metadata = path.metadata().unwrap();
        let codex = Codex;
        let chunk = codex
            .read_from(&session_for(&path, metadata.len()), 0)
            .unwrap();
        assert_eq!(chunk.events.len(), 1);
        assert!(chunk.next_offset < metadata.len());
    }

    #[test]
    fn extracts_patch_apply_paths() {
        let path = temp_path("jn4");
        let record = r#"{"timestamp":"2026-08-09T17:20:05.152Z","type":"event_msg","payload":{"type":"patch_apply_end","changes":{"/tmp/a.rs":{"type":"add"},"/tmp/b.rs":{"type":"update"}}}}"#;
        write_lines(&path, &[record]);
        let metadata = path.metadata().unwrap();
        let codex = Codex;
        let chunk = codex
            .read_from(&session_for(&path, metadata.len()), 0)
            .unwrap();
        assert_eq!(chunk.events.len(), 1);
        assert_eq!(chunk.events[0].paths.len(), 2);
    }

    #[test]
    fn discovers_a_forked_subagent_session_from_the_fixture() {
        let base = std::path::PathBuf::from("tests/fixtures/codex");
        let sessions = sessions_in(&base).unwrap();
        assert!(
            sessions.len() >= 2,
            "root and forked session both discovered"
        );
        let root = sessions
            .iter()
            .find(|session| session.parent.is_none())
            .expect("a root session with no forked_from_id");
        let child = sessions
            .iter()
            .find(|session| session.parent.as_deref() == Some(root.session_id.as_str()))
            .expect("a forked session naming the root as its parent");
        assert_ne!(root.session_id, child.session_id);
    }

    #[test]
    fn known_transcript_uses_persisted_metadata_without_parsing_its_first_record() {
        let base = temp_path("known-candidate");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let known_path = base.join("known.jsonl");
        let new_path = base.join("new.jsonl");
        write_lines(&known_path, &["not a codex session record"]);
        write_lines(
            &new_path,
            &[r#"{"type":"session_meta","payload":{"id":"new-session","cwd":"/tmp/new"}}"#],
        );
        let mut known = KnownSessions::new();
        known.insert(
            known_path.clone(),
            KnownSession {
                harness: HarnessId::Codex.as_str().to_owned(),
                session_id: "known-session".into(),
                nickname: "known-name".into(),
                cwd: Some("/tmp/known".into()),
                git_branch: None,
                parent: None,
                cursor: 23,
                modified_ms: 0,
                projection_version: 0,
            },
        );

        let sessions = sessions_in_with_known(&base, &known).unwrap();
        assert_eq!(sessions.len(), 2);
        let known = sessions
            .iter()
            .find(|session| session.path == known_path)
            .expect("known transcript candidate");
        assert_eq!(known.session_id, "known-session");
        assert_eq!(known.cwd.as_deref(), Some("/tmp/known"));
        assert!(sessions
            .iter()
            .any(|session| session.session_id == "new-session"));
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn codex_fixture_projects_through_the_graph_query() {
        let sessions = sessions_in(&std::path::PathBuf::from("tests/fixtures/codex")).unwrap();
        crate::harness::assert_fixture_sessions_project(&super::Codex, &sessions, 1);
    }

    #[test]
    fn parses_the_corpus_timestamp_shape() {
        assert_eq!(
            claude::parse_iso_ms("2026-08-09T17:20:05.152Z"),
            Some(1_786_296_005_152)
        );
    }

    // FAIL-PRE-FIX: call turns wrote `""`, and `_output` had no arm.
    #[test]
    fn custom_tool_call_projects_name_and_input() {
        let turns = project_one_line(
            "custom-tool-call",
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"response_item","payload":{"type":"custom_tool_call","name":"exec","input":"echo hi","call_id":"call_1"}}"#,
        );
        assert_eq!(turns[0]["role"], "tool");
        assert_eq!(turns[0]["said"].as_str().unwrap(), "exec\necho hi");
    }

    #[test]
    fn function_call_projects_name_and_arguments() {
        let turns = project_one_line(
            "function-call",
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"response_item","payload":{"type":"function_call","name":"shell","arguments":"{\"cmd\":\"ls\"}","call_id":"call_2"}}"#,
        );
        assert_eq!(
            turns[0]["said"].as_str().unwrap(),
            "shell\n{\"cmd\":\"ls\"}"
        );
    }

    #[test]
    fn custom_tool_call_output_joins_input_text_parts() {
        let turns = project_one_line(
            "custom-tool-call-output",
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"call_1","output":[{"type":"input_text","text":"line one"},{"type":"input_text","text":"line two"}]}}"#,
        );
        assert_eq!(turns[0]["role"], "tool");
        assert_eq!(turns[0]["said"].as_str().unwrap(), "line one\nline two");
    }

    #[test]
    fn function_call_output_keeps_the_raw_string() {
        let turns = project_one_line(
            "function-call-output",
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"response_item","payload":{"type":"function_call_output","call_id":"call_2","output":"total 0"}}"#,
        );
        assert_eq!(turns[0]["said"].as_str().unwrap(), "total 0");
    }

    #[test]
    fn agent_message_projects_under_the_assistant_role() {
        let turns = project_one_line(
            "agent-message",
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"event_msg","payload":{"type":"agent_message","message":"Working on it."}}"#,
        );
        assert_eq!(turns[0]["role"], "assistant");
        assert_eq!(turns[0]["said"].as_str().unwrap(), "Working on it.");
    }

    #[test]
    fn patch_apply_end_names_its_call_and_files() {
        let turns = project_one_line(
            "patch-apply-end",
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"event_msg","payload":{"type":"patch_apply_end","call_id":"exec-9","changes":{"/tmp/a.rs":{"type":"add"}}}}"#,
        );
        assert_eq!(
            turns[0]["said"].as_str().unwrap(),
            "patch exec-9\nfiles: /tmp/a.rs"
        );
    }

    /// RECEIPT (2026-08-25). A codex TUI pane showed four WARN lines per
    /// `boop` call: session_meta, task_started, world_state, turn_context
    /// each projected as raw json. Bookkeeping records leave no row and no
    /// warning; a reasoning summary lands as an assistant row.
    #[test]
    fn bookkeeping_records_leave_no_row_and_reasoning_keeps_its_summary() {
        for line in [
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"session_meta","payload":{"id":"s1","cwd":"/tmp"}}"#,
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"turn_context","payload":{"cwd":"/tmp","model":"gpt-test"}}"#,
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"world_state","payload":{"files":[]}}"#,
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}"#,
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"t1"}}"#,
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"event_msg","payload":{"type":"user_message","message":"hi"}}"#,
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"response_item","payload":{"type":"reasoning","summary":[]}}"#,
        ] {
            let turns = project_one_line("bookkeeping", line);
            assert!(turns.is_empty(), "{line}: {turns:?}");
        }
        let turns = project_one_line(
            "reasoning-summary",
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"response_item","payload":{"type":"reasoning","summary":[{"type":"summary_text","text":"Weighing two fixes."}]}}"#,
        );
        assert_eq!(turns[0]["role"], "assistant");
        assert_eq!(
            turns[0]["said"].as_str().unwrap(),
            "(reasoning)\nWeighing two fixes."
        );
    }

    /// RECEIPT (2026-08-28). A `--rebuild` sync over ~/.codex/sessions printed
    /// 24396 codex WARN lines, 9734 of them `sub_agent_activity` and 7935
    /// `agent_reasoning`, into the pane codex draws its TUI in. The four kinds
    /// below now project or stay silent.
    #[test]
    fn agent_reasoning_web_search_and_mcp_calls_project_instead_of_warning() {
        for line in [
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"event_msg","payload":{"type":"sub_agent_activity","agent_path":"/root/child","kind":"started"}}"#,
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"inter_agent_communication_metadata","payload":{"trigger_turn":true}}"#,
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"event_msg","payload":{"type":"turn_aborted","reason":"interrupted"}}"#,
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"event_msg","payload":{"type":"thread_rolled_back","num_turns":1}}"#,
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"event_msg","payload":{"type":"agent_reasoning","text":""}}"#,
        ] {
            let turns = project_one_line("codex-bookkeeping", line);
            assert!(turns.is_empty(), "{line}: {turns:?}");
        }

        let turns = project_one_line(
            "agent-reasoning",
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"event_msg","payload":{"type":"agent_reasoning","text":"**Planning the fix**"}}"#,
        );
        assert_eq!(turns[0]["role"], "assistant");
        assert_eq!(
            turns[0]["said"].as_str().unwrap(),
            "(reasoning)\n**Planning the fix**"
        );

        let turns = project_one_line(
            "web-search-end",
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"event_msg","payload":{"type":"web_search_end","query":"dbsp outer join","results":[{"type":"text_result"},{"type":"text_result"}]}}"#,
        );
        assert_eq!(turns[0]["role"], "tool");
        assert_eq!(
            turns[0]["said"].as_str().unwrap(),
            "web_search dbsp outer join\nresults: 2"
        );

        let turns = project_one_line(
            "web-search-call",
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"response_item","payload":{"type":"web_search_call","action":{"type":"search","query":"site:github.com hafley66"}}}"#,
        );
        assert_eq!(
            turns[0]["said"].as_str().unwrap(),
            "web_search site:github.com hafley66"
        );

        let turns = project_one_line(
            "mcp-tool-call-end",
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"event_msg","payload":{"type":"mcp_tool_call_end","invocation":{"server":"node_repl","tool":"js","arguments":{"code":"1+1"}}}}"#,
        );
        assert_eq!(turns[0]["role"], "tool");
        let body = turns[0]["said"].as_str().unwrap();
        assert!(body.starts_with("node_repl__js\n"), "{body}");
        assert!(body.contains("1+1"), "{body}");
    }

    /// RECEIPT. The second sighting of one unprojected kind is a debug event,
    /// so a session with 9734 of them costs the pane one line.
    #[test]
    fn an_unprojected_kind_warns_once_per_process() {
        let label = "a_kind_only_this_test_names";
        assert!(super::super::first_projection_gap("codex", label));
        assert!(!super::super::first_projection_gap("codex", label));
        assert!(super::super::first_projection_gap("opencode", label));
    }

    #[test]
    fn an_unknown_record_type_keeps_its_raw_json() {
        let turns = project_one_line(
            "unknown-record",
            r#"{"timestamp":"2026-08-09T17:20:05.000Z","type":"event_msg","payload":{"type":"a_kind_from_the_future","payload_field":"keep me"}}"#,
        );
        let body = turns[0]["said"].as_str().unwrap();
        assert!(body.starts_with("a_kind_from_the_future (unprojected)"));
        assert!(body.contains("keep me"));
    }

    #[test]
    fn tool_call_body_truncates_input_to_2000_chars() {
        let payload = serde_json::json!({ "input": "a".repeat(3000) });
        let body = super::tool_call_body("exec", payload.as_object().unwrap());
        let (name, input) = body.split_once('\n').unwrap();
        assert_eq!(name, "exec");
        assert_eq!(input.len(), 2000);
    }

    #[test]
    fn tool_output_body_truncates_output_to_4000_chars() {
        let payload = serde_json::json!({ "output": "b".repeat(5000) });
        let body = super::tool_output_body(payload.as_object().unwrap());
        assert_eq!(body.len(), 4000);
    }

    /// RECEIPT. No tool or assistant turn the fixture projects is empty.
    #[test]
    fn codex_fixture_tool_and_assistant_turns_keep_their_content() {
        let fixture = PathBuf::from(
            "tests/fixtures/codex/2026/08/24/\
             rollout-2026-08-24T22-10-59-01a036af-654b-72c3-b468-3266bc459b4e.jsonl",
        );
        let size = std::fs::metadata(&fixture).unwrap().len();
        let session = session_for(&fixture, size);
        let store_path = temp_path("fixture-tool-bodies-store");
        let _ = std::fs::remove_file(&store_path);
        let store = Store::open(store_path.clone()).unwrap();
        sync_session(&store, &Codex, &session).unwrap();
        let turns = store
            .query_turns(&TurnQuery {
                session: Some(session.session_id.clone()),
                ..TurnQuery::default()
            })
            .unwrap();
        assert!(!turns.is_empty(), "the fixture projects turns");

        let empty_tool = turns
            .iter()
            .filter(|turn| {
                turn["role"] == "tool" && turn["said"].as_str().unwrap_or_default().is_empty()
            })
            .count();
        let empty_assistant = turns
            .iter()
            .filter(|turn| {
                turn["role"] == "assistant" && turn["said"].as_str().unwrap_or_default().is_empty()
            })
            .count();
        assert_eq!(empty_tool, 0, "empty tool bodies: {turns:#?}");
        assert_eq!(empty_assistant, 0, "empty assistant bodies: {turns:#?}");

        println!("projected tool rows, calls and results (first 3):");
        let named_tool_rows = turns.iter().filter(|turn| {
            turn["role"] == "tool"
                && !turn["said"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("(unprojected)")
        });
        for row in named_tool_rows.take(3) {
            println!("{}", serde_json::to_string_pretty(row).unwrap());
        }

        drop(store);
        let _ = std::fs::remove_file(store_path);
    }
}
