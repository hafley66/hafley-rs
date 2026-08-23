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
use boop_store::session::ModelSpec;
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
};

/// The state database and remote-control socket of the codex on this machine.
static DOOR: crate::door::codex::CodexDoor = crate::door::codex::CodexDoor::machine();

impl Harness for Codex {
    fn identity_process(&self) -> Option<crate::identity::Identity> {
        let session = std::env::var("CODEX_THREAD_ID")
            .ok()
            .filter(|value| !value.is_empty())?;
        let pane = std::env::var("TMUX_PANE")
            .ok()
            .filter(|value| !value.is_empty())
            .or_else(|| boop_store::tmux::mux().current_pane(None))?;
        Some(crate::identity::Identity {
            session: Some(session),
            lane: Some(format!("codex-{}", pane.trim_start_matches('%'))),
            harness: Some(self.id().to_string()),
            pane: Some(pane),
            rung: Some(crate::identity::Rung::CodexProcess),
            ..Default::default()
        })
    }

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

    fn live(&self) -> &dyn crate::live::LiveSessions {
        &DOOR
    }

    fn door(&self) -> &dyn crate::door::Door {
        &DOOR
    }

    /// `send_midflight` stays false: `codex exec` reads no stdin mid-turn,
    /// and interactive codex never exits, so the on-exit hail never fires.
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

/// `codex exec`, never interactive: the on-exit hail rides process exit and
/// interactive codex idles forever (class 42). `@effort` -> reasoning config.
fn launch_command(spec: &SpawnSpec) -> anyhow::Result<String> {
    let mut command = match &spec.resume_session {
        Some(id) => format!(
            "codex exec resume {} {}",
            shell_quote(id),
            shell_quote(&spec.prompt)
        ),
        None => format!("codex exec {}", shell_quote(&spec.prompt)),
    };
    command.push_str(" --dangerously-bypass-approvals-and-sandbox");
    if let Some(model) = spec.model.as_deref().filter(|value| !value.is_empty()) {
        let model_spec: ModelSpec = model.parse()?;
        command.push_str(&format!(" -m {}", shell_quote(&model_spec.name)));
        if let Some(effort) = model_spec.effort {
            command.push_str(&format!(
                " -c {}",
                shell_quote(&format!("model_reasoning_effort=\"{}\"", effort.as_str()))
            ));
        }
    }
    Ok(spec.with_on_exit(match &spec.env_stamp {
        Some(stamp) => format!("{stamp} {command}"),
        None => command,
    }))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
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

#[allow(clippy::too_many_arguments)]
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
            let inserted = store.write_turn(&sid, *turn, ts, "tool", "")?;
            record(stat, inserted);
            store.write_tool_fact(&sid, *turn, ts, name, None)?;
        }
        "patch_apply_end" => {
            let Some(changes) = payload.get("changes").and_then(Value::as_object) else {
                return Ok(());
            };
            if changes.is_empty() {
                return Ok(());
            }
            *turn += 1;
            let inserted = store.write_turn(&sid, *turn, ts, "tool", "")?;
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
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::harness::HarnessId;
    use std::path::PathBuf;

    use std::fs::OpenOptions;
    use std::io::Write;

    use crate::harness::{claude, Harness, KnownSession, KnownSessions, SessionRef};
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

    #[test]
    fn codex_capabilities_are_measured() {
        let caps = Codex.control_capabilities();
        assert!(!caps.send_midflight, "codex exec reads no stdin mid-turn");
        assert!(caps.resume);
        assert!(caps.spawn);
        assert!(caps.subagent_visible);
    }

    #[test]
    fn launch_command_resumes_by_session_id() {
        let mut spec = spawn_spec(None);
        spec.resume_session = Some("0192aef3-aaaa-bbbb-cccc-dddddddddddd".to_owned());
        spec.model = Some("gpt-5.6-luna".to_owned());
        let command = super::launch_command(&spec).unwrap();
        assert!(command.starts_with("codex exec resume "), "{command}");
        assert!(command.contains("'do the lane'"), "{command}");
        assert!(
            command.contains(" --dangerously-bypass-approvals-and-sandbox"),
            "{command}"
        );
        assert!(command.ends_with(" -m 'gpt-5.6-luna'"), "{command}");
    }

    #[test]
    fn launch_command_passes_model_and_effort_suffix() {
        let mut spec = spawn_spec(None);
        spec.model = Some("gpt-5.6-luna@medium".to_owned());
        let command = super::launch_command(&spec).unwrap();
        assert!(command.starts_with("codex exec 'do the lane'"), "{command}");
        assert!(command.contains(" -m 'gpt-5.6-luna'"), "{command}");
        assert!(
            command.contains(" -c 'model_reasoning_effort=\"medium\"'"),
            "{command}"
        );
    }

    #[test]
    fn launch_command_leaves_plain_model_alone() {
        let mut spec = spawn_spec(None);
        spec.model = Some("gpt-5.6-sol".to_owned());
        let command = super::launch_command(&spec).unwrap();
        assert!(command.ends_with(" -m 'gpt-5.6-sol'"), "{command}");
        assert!(!command.contains("model_reasoning_effort"), "{command}");
    }

    /// RECEIPT. Pre-fix, an `@` suffix outside the effort allowlist stayed
    /// glued to the model name and was passed to codex unvalidated, failing
    /// downstream inside the codex binary instead of here. `x@turbo` now
    /// fails at parse, naming the allowlist.
    #[test]
    fn launch_command_rejects_an_at_suffix_outside_the_effort_allowlist() {
        let mut spec = spawn_spec(None);
        spec.model = Some("vendor@custom".to_owned());
        let error = super::launch_command(&spec).unwrap_err().to_string();
        assert!(error.contains("low"), "message: {error}");
        assert!(error.contains("medium"), "message: {error}");
        assert!(error.contains("high"), "message: {error}");
    }

    fn spawn_spec(socket: Option<String>) -> crate::harness::SpawnSpec {
        crate::harness::SpawnSpec {
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
}
