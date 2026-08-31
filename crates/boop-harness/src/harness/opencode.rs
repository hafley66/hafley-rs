//! The opencode adapter. opencode writes no transcript file, so this tails
//! `message.rowid` in its SQLite store, read-only; opencode owns that store.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde_json::Value;

use crate::harness::{
    Capabilities, ControlCapabilities, Harness, HarnessId, Ingested, KnownSessions, LanePolicy,
    MailPolicy, OneShotSpec, ReadChunk, SessionRef, SpawnSpec, VariantSupport,
};
use boop_store::event::AgentEvent;
use boop_store::ident::{Store, SyncStat, UsageRow};

pub struct Opencode;

/// The `provider/model` form is opencode's, so its prefix list is the empty
/// prefix; plan-family models would bill metered credit and are refused.
static CAPABILITIES: Capabilities = Capabilities {
    bans_plan_family_models: true,
    lanes: LanePolicy::Allowed,
    variant: VariantSupport::Flag,
    mail: MailPolicy::Door,
    native_tui_projector: true,
    wrapper_owns_alternate_screen: false,
};

/// The `opencode serve` this machine's TUIs are clients of.
static DOOR: crate::door::opencode::OpencodeDoor = crate::door::opencode::OpencodeDoor::machine();

impl Harness for Opencode {
    fn open_channel(
        &self,
        spec: &boop_acp::channel::ChannelSpec,
    ) -> anyhow::Result<Box<dyn boop_acp::channel::LaneChannel>> {
        Ok(Box::new(boop_acp::channel::acp::AcpChannel::open_adapter(
            spec,
            boop_acp::channel::acp::OPENCODE_ADAPTER,
        )?))
    }

    fn id(&self) -> HarnessId {
        HarnessId::Opencode
    }

    fn capabilities(&self) -> &'static Capabilities {
        &CAPABILITIES
    }

    fn tui_composer(&self) -> crate::harness::TuiComposer {
        crate::harness::TuiComposer::Opencode
    }

    fn live(&self) -> &dyn crate::live::LiveSessions {
        &DOOR
    }

    fn door(&self) -> &dyn crate::door::Door {
        &DOOR
    }

    fn sessions(&self) -> Result<Vec<SessionRef>> {
        let Some(path) = store_path() else {
            return Ok(Vec::new());
        };
        sessions_from(&path)
    }

    fn sync_candidate(
        &self,
        known: &KnownSessions,
        session_id: &str,
    ) -> Result<Option<SessionRef>> {
        let Some((path, _)) = known.find(self.id().as_str(), session_id) else {
            return Ok(None);
        };
        session_from(path, session_id)
    }

    fn session_roots(&self) -> Result<Vec<PathBuf>> {
        Ok(store_path().into_iter().collect())
    }

    fn read_from(&self, session: &SessionRef, offset: u64) -> Result<ReadChunk> {
        let connection = open_read_only(&session.path)?;
        let mut events = Vec::new();
        let mut next = offset;
        for message in messages_after(&connection, &session.session_id, offset)? {
            next = message.rowid;
            events.push(AgentEvent {
                harness: HarnessId::Opencode.as_str(),
                session_id: session.session_id.clone(),
                ts_ms: message.ts,
                uuid: Some(message.id.clone()),
                parent_uuid: None,
                cwd: session.cwd.clone(),
                git_branch: None,
                record_type: message.role.clone(),
                tool_name: None,
                paths: Vec::new(),
                urls: Vec::new(),
                raw_line_offset: message.rowid,
            });
        }
        Ok(ReadChunk {
            events,
            next_offset: next,
            reset: false,
            skipped: 0,
        })
    }

    /// `send_midflight` stays false: `opencode run` reads no stdin mid-turn,
    /// so a pane injection lands on dead air (transport itself is tested).
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

    fn one_shot(&self, spec: &OneShotSpec) -> Result<String> {
        let model = spec
            .model
            .as_deref()
            .filter(|value| !value.is_empty())
            .context("one-shot spec has no model; opencode needs one resolved")?;
        // A wedged `opencode run` must not wedge the caller: poll the child and
        // kill it past the guard instead of blocking on output() forever.
        const GUARD: Duration = Duration::from_secs(600);
        let mut child = std::process::Command::new("opencode")
            .args(["run", "-m", model, &spec.prompt])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("spawn opencode run")?;
        let started = std::time::Instant::now();
        let status = loop {
            match child.try_wait().context("poll opencode run")? {
                Some(status) => break status,
                None if started.elapsed() >= GUARD => {
                    let _ = child.kill();
                    anyhow::bail!("opencode run -m {model} exceeded 600s guard, killed");
                }
                None => std::thread::sleep(Duration::from_millis(500)),
            }
        };
        let output = child
            .wait_with_output()
            .context("collect opencode run output")?;
        if !status.success() {
            anyhow::bail!(
                "opencode run -m {model} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    fn spawn(&self, spec: &SpawnSpec) -> Result<SessionRef> {
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
            harness: HarnessId::Opencode,
            session_id: session_id.clone(),
            nickname: session_id,
            path: opencode_db_path().unwrap_or_else(|| cwd.join("opencode.db")),
            cwd: Some(cwd.display().to_string()),
            git_branch: Some(spec.branch.clone()),
            modified_ms: now_ms(),
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

    fn ingest(&self, store: &Store, session: &SessionRef, from: u64) -> Result<Ingested> {
        let connection = open_read_only(&session.path)?;
        let messages = messages_after(&connection, &session.session_id, from)?;
        if messages.is_empty() {
            return Ok(Ingested {
                stat: SyncStat::default(),
                next_cursor: from,
            });
        }
        let mut turn = store.begin_walk(&session.session_id)?;
        let mut stat = SyncStat::default();
        let mut current_message = 0;
        let mut active_message = false;
        let mut first_turn = None;
        visit_parts_for_messages(&connection, &messages, |message_index, part| {
            if !active_message {
                for message in messages.iter().take(message_index) {
                    finish_message(
                        store,
                        &session.session_id,
                        message,
                        &mut first_turn,
                        &mut turn,
                        &mut stat,
                    )?;
                }
                current_message = message_index;
                active_message = true;
            } else if message_index != current_message {
                finish_message(
                    store,
                    &session.session_id,
                    &messages[current_message],
                    &mut first_turn,
                    &mut turn,
                    &mut stat,
                )?;
                for message in messages
                    .iter()
                    .take(message_index)
                    .skip(current_message + 1)
                {
                    first_turn = None;
                    finish_message(
                        store,
                        &session.session_id,
                        message,
                        &mut first_turn,
                        &mut turn,
                        &mut stat,
                    )?;
                }
                current_message = message_index;
                first_turn = None;
            }
            write_part(
                store,
                &session.session_id,
                &messages[message_index],
                &part,
                &mut turn,
                &mut first_turn,
                &mut stat,
            )
        })?;
        if active_message {
            finish_message(
                store,
                &session.session_id,
                &messages[current_message],
                &mut first_turn,
                &mut turn,
                &mut stat,
            )?;
            current_message += 1;
        }
        while current_message < messages.len() {
            first_turn = None;
            finish_message(
                store,
                &session.session_id,
                &messages[current_message],
                &mut first_turn,
                &mut turn,
                &mut stat,
            )?;
            current_message += 1;
        }
        let cursor = messages.last().map(|message| message.rowid).unwrap_or(from);
        Ok(Ingested {
            stat,
            next_cursor: cursor,
        })
    }
}

/// The last message rowid opencode holds per session. `size` is what the sync
/// freshness gate compares against the stored cursor, and this harness's
/// cursor IS a message rowid, so reporting anything else marks every session
/// that has ever been synced as pending on every pass.
fn last_message_rowid(connection: &Connection) -> Result<HashMap<String, u64>> {
    let mut statement =
        connection.prepare("SELECT session_id, MAX(rowid) FROM message GROUP BY session_id")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut out = HashMap::new();
    for row in rows {
        let (session, rowid) = row?;
        out.insert(session, rowid as u64);
    }
    Ok(out)
}

fn sessions_from(path: &std::path::Path) -> Result<Vec<SessionRef>> {
    let connection = open_read_only(path)?;
    let last_rowid = last_message_rowid(&connection)?;
    let mut statement = connection.prepare(
        "SELECT id, directory, parent_id, slug, time_updated FROM session ORDER BY time_updated",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;
    let mut sessions = Vec::new();
    for row in rows {
        let (id, directory, parent, slug, updated) = row?;
        let size = last_rowid.get(&id).copied().unwrap_or(0);
        sessions.push(SessionRef {
            harness: HarnessId::Opencode,
            session_id: id.clone(),
            nickname: slug.unwrap_or(id),
            path: path.to_owned(),
            cwd: directory,
            git_branch: None,
            modified_ms: updated as u64,
            size,
            tmux: None,
            tmux_socket: None,
            parent,
        });
    }
    Ok(sessions)
}

fn session_from(path: &std::path::Path, session_id: &str) -> Result<Option<SessionRef>> {
    let connection = open_read_only(path)?;
    connection
        .query_row(
            "SELECT id, directory, parent_id, slug, time_updated,
                    COALESCE((SELECT MAX(rowid) FROM message WHERE session_id = session.id), 0)
               FROM session WHERE id = ?1",
            [session_id],
            |row| {
                let id = row.get::<_, String>(0)?;
                Ok(SessionRef {
                    harness: HarnessId::Opencode,
                    session_id: id.clone(),
                    nickname: row.get::<_, Option<String>>(3)?.unwrap_or(id),
                    path: path.to_owned(),
                    cwd: row.get(1)?,
                    git_branch: None,
                    modified_ms: row.get::<_, i64>(4)? as u64,
                    size: row.get::<_, i64>(5)? as u64,
                    tmux: None,
                    tmux_socket: None,
                    parent: row.get(2)?,
                })
            },
        )
        .optional()
        .context("query one opencode session")
}

fn record(stat: &mut SyncStat, inserted: usize) {
    if inserted == 0 {
        stat.dropped += 1;
    } else {
        stat.written += 1;
    }
}

fn write_part(
    store: &Store,
    session_id: &str,
    message: &Message,
    part: &Part,
    turn: &mut u64,
    first_turn: &mut Option<u64>,
    stat: &mut SyncStat,
) -> Result<()> {
    match part.kind.as_str() {
        "text" => {
            *turn += 1;
            let inserted =
                store.write_turn(session_id, *turn, message.ts, &message.role, &part.text)?;
            record(stat, inserted);
            first_turn.get_or_insert(*turn);
        }
        "tool" => {
            *turn += 1;
            let inserted =
                store.write_turn(session_id, *turn, message.ts, "tool", &part.tool_body())?;
            record(stat, inserted);
            first_turn.get_or_insert(*turn);
            store.write_tool_fact(
                session_id,
                *turn,
                message.ts,
                &part.tool,
                part.input.as_ref(),
            )?;
        }
        // The model's own words before a tool call. Projected under the
        // message's role so a chat read shows what the lane was thinking
        // rather than 48 empty assistant rows.
        "reasoning" => {
            *turn += 1;
            let inserted = store.write_turn(
                session_id,
                *turn,
                message.ts,
                &message.role,
                &format!("reasoning: {}", part.text),
            )?;
            record(stat, inserted);
            first_turn.get_or_insert(*turn);
        }
        // A snapshot of the files one step wrote. The hash and the paths are
        // the content; there is no prose to keep.
        "patch" => {
            *turn += 1;
            let inserted =
                store.write_turn(session_id, *turn, message.ts, "tool", &part.patch_body())?;
            record(stat, inserted);
            first_turn.get_or_insert(*turn);
        }
        // A file the user attached to the message with `@path`. The path is
        // the content, and it counts as a read of that file.
        "file" => {
            let path = part.file_path();
            *turn += 1;
            let inserted = store.write_turn(
                session_id,
                *turn,
                message.ts,
                &message.role,
                &format!("file {path}"),
            )?;
            record(stat, inserted);
            first_turn.get_or_insert(*turn);
            if !path.is_empty() {
                store.write_tool_fact(
                    session_id,
                    *turn,
                    message.ts,
                    "Read",
                    Some(&serde_json::json!({ "file_path": path })),
                )?;
            }
        }
        // Step markers carry no readable content of their own. Their token
        // counts reach the store through `finish_message`.
        kind if STRUCTURAL_PARTS.contains(&kind) => {}
        // A kind this projection has never seen. The raw JSON becomes the body
        // rather than disappearing, and the gap is reported so the adapter can
        // grow a real arm for it.
        kind => {
            *turn += 1;
            let inserted =
                store.write_turn(session_id, *turn, message.ts, "tool", &part.gap_body())?;
            record(stat, inserted);
            first_turn.get_or_insert(*turn);
            // One line per kind per process; the pane an opencode TUI draws
            // in is the same fd this writes to.
            if crate::harness::first_projection_gap("opencode", kind) {
                tracing::warn!(
                    projection_gap = kind,
                    session_id,
                    turn = *turn,
                    "opencode part kind projected as raw json"
                );
            } else {
                tracing::debug!(
                    projection_gap = kind,
                    session_id,
                    turn = *turn,
                    "opencode part kind projected as raw json"
                );
            }
        }
    }
    Ok(())
}

/// Part kinds whose raw event holds no readable content. Every other kind
/// projects a body, so an empty body always means an empty event.
const STRUCTURAL_PARTS: [&str; 4] = ["step-start", "step-finish", "snapshot", "compaction"];

fn finish_message(
    store: &Store,
    session_id: &str,
    message: &Message,
    first_turn: &mut Option<u64>,
    turn: &mut u64,
    stat: &mut SyncStat,
) -> Result<()> {
    let Some(usage) = message.usage() else {
        return Ok(());
    };
    let attach = match first_turn {
        Some(turn) => *turn,
        None => {
            *turn += 1;
            let inserted = store.write_turn(session_id, *turn, message.ts, &message.role, "")?;
            record(stat, inserted);
            *turn
        }
    };
    let (is_new, changed) = store.write_usage(session_id, attach, &usage)?;
    if changed {
        if is_new {
            stat.usage_written += 1;
        } else {
            stat.usage_updated += 1;
        }
    }
    Ok(())
}

/// One opencode message, with the token counts it records.
pub struct Message {
    pub rowid: u64,
    pub id: String,
    pub ts: u64,
    pub role: String,
    data: Value,
}

impl Message {
    /// opencode records no request id, so the dedup key is the message id
    /// alone; reasoning tokens are billed as output and fold into it.
    pub fn usage(&self) -> Option<UsageRow<'_>> {
        if self.role != "assistant" {
            return None;
        }
        let tokens = self.data.get("tokens")?.as_object()?;
        let count = |key: &str| -> i64 { tokens.get(key).and_then(Value::as_i64).unwrap_or(0) };
        let cache = |key: &str| -> i64 {
            tokens
                .get("cache")
                .and_then(Value::as_object)
                .and_then(|cache| cache.get(key))
                .and_then(Value::as_i64)
                .unwrap_or(0)
        };
        Some(UsageRow {
            ts: self.ts,
            message_id: &self.id,
            request_id: "",
            model: self
                .data
                .get("modelID")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            service_tier: None,
            input_tokens: count("input"),
            output_tokens: count("output") + count("reasoning"),
            cache_create_5m_tokens: cache("write"),
            cache_create_1h_tokens: 0,
            cache_read_tokens: cache("read"),
            is_sidechain: false,
            cost_usd_recorded: self.data.get("cost").and_then(Value::as_f64),
        })
    }
}

/// One content part of a message.
pub struct Part {
    pub kind: String,
    pub tool: String,
    pub text: String,
    pub input: Option<Value>,
    pub output: Option<Value>,
    pub error: Option<Value>,
    /// A `patch` part's snapshot hash and the paths it wrote.
    pub hash: String,
    pub files: Vec<String>,
    /// The event exactly as opencode wrote it, so a kind with no arm still
    /// reaches the chat body instead of vanishing.
    pub raw: String,
}

impl Part {
    /// The files one step wrote, named by the snapshot that holds them.
    fn patch_body(&self) -> String {
        let files = self
            .files
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        match files.is_empty() {
            true => format!("patch {}", self.hash),
            false => format!("patch {}\nfiles: {files}", self.hash),
        }
    }

    /// The path a `file` part names. `filename` is the relative spelling the
    /// user typed; `source.path` is the fallback when it is absent.
    fn file_path(&self) -> String {
        let Ok(raw) = serde_json::from_str::<Value>(&self.raw) else {
            return String::new();
        };
        raw.get("filename")
            .and_then(Value::as_str)
            .or_else(|| {
                raw.get("source")
                    .and_then(|source| source.get("path"))
                    .and_then(Value::as_str)
            })
            .unwrap_or_default()
            .to_owned()
    }

    /// An unknown kind's body: the raw event, verbatim, under its own name.
    fn gap_body(&self) -> String {
        format!("{} (unprojected)\n{}", self.kind, self.raw)
    }

    /// The chat body holds the complete readable tool exchange. Structured
    /// facts remain separately projected for queries over commands and paths.
    fn tool_body(&self) -> String {
        let mut body = format!("tool {}", self.tool);
        if let Some(input) = &self.input {
            body.push_str("\ninput: ");
            body.push_str(&input.to_string());
        }
        if let Some(output) = &self.output {
            body.push_str("\noutput: ");
            body.push_str(&output.to_string());
        }
        if let Some(error) = &self.error {
            body.push_str("\nerror: ");
            body.push_str(&error.to_string());
        }
        body
    }
}

/// The opencode store on this machine, `None` until opencode has created it.
pub fn store_path() -> Option<PathBuf> {
    opencode_db_path().filter(|path| path.exists())
}

/// The db path regardless of existence; a spawn writes its command before
/// opencode has ever created the file.
pub fn opencode_db_path() -> Option<PathBuf> {
    Some(
        dirs::home_dir()?
            .join(".local")
            .join("share")
            .join("opencode")
            .join("opencode.db"),
    )
}

/// The opencode command a spawn runs. Opencode has no default model; the
/// caller resolves one into `spec.model` or the spawn refuses.
#[allow(dead_code)] // spawn() runs supervisor_command instead; kept live by its own tests below.
fn launch_command(spec: &SpawnSpec) -> Result<String> {
    let model = spec
        .model
        .as_deref()
        .filter(|value| !value.is_empty())
        .context("spawn spec has no model; opencode needs one resolved by the caller")?;
    let mut command = format!("opencode run -m {}", shell_quote(model));
    if let Some(variant) = spec.variant.as_deref().filter(|value| !value.is_empty()) {
        command.push_str(&format!(" --variant {}", shell_quote(variant)));
    }
    if let Some(session) = &spec.resume_session {
        command.push_str(&format!(" -s {}", shell_quote(session)));
    }
    command.push_str(&format!(
        " --auto \"$(cat {})\"",
        shell_quote_double(&spec.prompt)
    ));
    Ok(spec.with_on_exit(match &spec.env_stamp {
        Some(stamp) => format!("{stamp} {command}"),
        None => command,
    }))
}

#[allow(dead_code)] // only called from launch_command, itself dead code (see above).
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Double-quote a value for use inside an already-double-quoted `$(cat ...)`
/// substitution; bash nests nested `"..."` correctly inside `$(...)`.
#[allow(dead_code)] // only called from launch_command, itself dead code (see above).
fn shell_quote_double(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

// The old per-byte time sample repeated one byte 8 times (measured live:
// "62626262626a6a6a"), so two close spawns could collide.
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

/// Read-only, and never creating the file: a missing opencode is "no sessions",
/// never an empty store this process invented.
fn open_read_only(path: &std::path::Path) -> Result<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open opencode store at {}", path.display()))
}

/// Messages after a rowid cursor. rowid is the resume point because it rises
/// with insertion order and two messages can share a millisecond.
fn messages_after(connection: &Connection, session: &str, after: u64) -> Result<Vec<Message>> {
    let mut statement = connection.prepare(
        "SELECT rowid, id, time_created, data FROM message
         WHERE session_id = ?1 AND rowid > ?2 ORDER BY rowid",
    )?;
    let rows = statement.query_map(rusqlite::params![session, after as i64], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (rowid, id, ts, raw) = row?;
        let data: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
        let role = data
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        out.push(Message {
            rowid: rowid as u64,
            id,
            ts: ts as u64,
            role,
            data,
        });
    }
    Ok(out)
}

/// Visit every part for a message batch in one statement, retaining message
/// order and part id order while keeping only the current row in memory.
/// SQLite's host-parameter limit varies by build, so large message batches
/// are split before preparing the IN query.
fn visit_parts_for_messages<F>(
    connection: &Connection,
    messages: &[Message],
    mut visit: F,
) -> Result<()>
where
    F: FnMut(usize, Part) -> Result<()>,
{
    const MAX_MESSAGE_IDS_PER_QUERY: usize = 500;

    for (batch_index, message_batch) in messages.chunks(MAX_MESSAGE_IDS_PER_QUERY).enumerate() {
        let message_indexes: HashMap<&str, usize> = message_batch
            .iter()
            .enumerate()
            .map(|(index, message)| (message.id.as_str(), index))
            .collect();
        let placeholders = (1..=message_batch.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let order = (1..=message_batch.len())
            .map(|index| format!("WHEN ?{index} THEN {}", index - 1))
            .collect::<Vec<_>>()
            .join(" ");
        let sql = format!(
            "SELECT message_id, data FROM part
             WHERE message_id IN ({placeholders})
             ORDER BY CASE message_id {order} END, id"
        );
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map(
            rusqlite::params_from_iter(message_batch.iter().map(|message| message.id.as_str())),
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?;
        for row in rows {
            let (message_id, raw) = row?;
            let Some(&message_index) = message_indexes.get(message_id.as_str()) else {
                continue;
            };
            let data: Value = match serde_json::from_str(&raw) {
                Ok(value) => value,
                Err(_) => continue,
            };
            visit(
                batch_index * MAX_MESSAGE_IDS_PER_QUERY + message_index,
                Part {
                    kind: data
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    tool: data
                        .get("tool")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    text: data
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    input: data
                        .get("state")
                        .and_then(|state| state.get("input"))
                        .cloned(),
                    output: data
                        .get("state")
                        .and_then(|state| state.get("output"))
                        .cloned(),
                    error: data
                        .get("state")
                        .and_then(|state| state.get("error"))
                        .cloned(),
                    hash: data
                        .get("hash")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    files: data
                        .get("files")
                        .and_then(Value::as_array)
                        .map(|files| {
                            files
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_owned)
                                .collect()
                        })
                        .unwrap_or_default(),
                    raw,
                },
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::harness::HarnessId;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use rusqlite::trace::{TraceEvent, TraceEventCodes};

    use super::{
        launch_command, messages_after, sessions_from, visit_parts_for_messages, Opencode, Part,
    };
    use crate::harness::{sync_session, Harness, KnownSessions, SpawnSpec};
    use boop_store::ident::{Store, TurnQuery};
    use boop_store::testing::TempRepo;

    static PART_SELECTS: AtomicUsize = AtomicUsize::new(0);

    fn count_part_selects(event: TraceEvent<'_>) {
        if let TraceEvent::Stmt(_, sql) = event {
            if sql.contains("FROM part") {
                PART_SELECTS.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Capabilities are claims; each true one needs a test.
    #[test]
    fn opencode_capabilities_match_the_binary() {
        let caps = Opencode.control_capabilities();
        assert!(!caps.send_midflight, "opencode run reads no stdin mid-turn");
        assert!(caps.resume, "opencode run -s <sessionID> resumes");
        assert!(caps.spawn, "spawn is implemented and tested below");
        assert!(caps.subagent_visible, "session.parent_id names the parent");
    }

    /// RECEIPT (2026-08-28). `file` and `compaction` were the last two kinds a
    /// `--rebuild` sync still projected as raw json. A `file` part names the
    /// path the user attached; a `compaction` part carries no readable text.
    #[test]
    fn a_file_part_names_its_path_and_compaction_is_structural() {
        let file_part = |raw: &str| Part {
            kind: "file".to_owned(),
            tool: String::new(),
            text: String::new(),
            input: None,
            output: None,
            error: None,
            hash: String::new(),
            files: Vec::new(),
            raw: raw.to_owned(),
        };
        assert_eq!(
            file_part(
                r#"{"type":"file","mime":"text/plain","filename":"chat_log/a.md","source":{"type":"file","path":"chat_log/a.md"}}"#
            )
            .file_path(),
            "chat_log/a.md"
        );
        assert_eq!(
            file_part(r#"{"type":"file","source":{"type":"file","path":"docs/b.md"}}"#).file_path(),
            "docs/b.md"
        );

        assert!(super::STRUCTURAL_PARTS.contains(&"compaction"));
    }

    /// A missing opencode store is no sessions, never an error and never a
    /// store this process created.
    #[test]
    fn a_missing_store_is_no_sessions() {
        assert!(super::open_read_only(std::path::Path::new("/tmp/boop-no-such.db")).is_err());
    }

    #[test]
    fn opencode_fixture_acquisition_and_parent_project_through_graph() {
        let fixture = sessions_from(std::path::Path::new(
            "tests/fixtures/opencode/bench/opencode.db",
        ))
        .unwrap();
        assert_eq!(fixture.len(), 2);

        let path =
            std::env::temp_dir().join(format!("boop-opencode-parent-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::fs::copy("tests/fixtures/opencode/bench/opencode.db", &path).unwrap();
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute(
                "INSERT INTO session(id, directory, parent_id, slug, time_updated)
                 VALUES ('ses_parent_fixture', '/bench/opencode', NULL, 'parent', 1),
                        ('ses_child_fixture', '/bench/opencode', 'ses_parent_fixture', 'child', 2)",
                [],
            )
            .unwrap();
        let sessions = sessions_from(&path).unwrap();
        crate::harness::assert_fixture_sessions_project(&Opencode, &sessions, 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn exact_sync_candidate_refreshes_the_session_rowid_cursor() {
        let path = std::env::temp_dir().join(format!(
            "boop-opencode-exact-session-{}-{:?}.db",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&path);
        std::fs::copy("tests/fixtures/opencode/bench/opencode.db", &path).unwrap();
        let initial = sessions_from(&path)
            .unwrap()
            .into_iter()
            .find(|session| session.session_id == "ses_bench_0001")
            .unwrap();
        let mut known = KnownSessions::new();
        known.upsert_ref(&initial, initial.size);

        let refreshed = Opencode
            .sync_candidate(&known, &initial.session_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                refreshed.session_id.as_str(),
                refreshed.nickname.as_str(),
                refreshed.cwd.as_deref(),
                refreshed.parent.as_deref(),
                refreshed.size,
            ),
            (
                initial.session_id.as_str(),
                initial.nickname.as_str(),
                initial.cwd.as_deref(),
                initial.parent.as_deref(),
                initial.size,
            )
        );
        assert_ne!(refreshed.size, std::fs::metadata(&path).unwrap().len());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_part_with_no_state_still_parses() {
        let part = Part {
            kind: "text".to_owned(),
            tool: String::new(),
            text: "hello".to_owned(),
            input: None,
            output: None,
            error: None,
            hash: String::new(),
            files: Vec::new(),
            raw: String::new(),
        };
        assert_eq!(part.text, "hello");
    }

    #[test]
    fn opencode_projection_preserves_assistant_and_complete_tool_bodies() {
        let source = std::path::Path::new("tests/fixtures/opencode/bench/opencode.db");
        let transcript = std::env::temp_dir().join(format!(
            "boop-opencode-body-{}-{:?}.db",
            std::process::id(),
            std::thread::current().id()
        ));
        let store_path = std::env::temp_dir().join(format!(
            "boop-opencode-body-store-{}-{:?}.db",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&transcript);
        let _ = std::fs::remove_file(&store_path);
        std::fs::copy(source, &transcript).unwrap();
        rusqlite::Connection::open(&transcript)
            .unwrap()
            .execute(
                "UPDATE part SET data = ?1 WHERE id = 'ses_bench_0001_msg_0005_p1'",
                [r#"{"type":"tool","tool":"bash","state":{"input":{"command":"cargo test bench_5"},"output":"tests passed","error":"stderr empty"}}"#],
            )
            .unwrap();

        let session = sessions_from(&transcript)
            .unwrap()
            .into_iter()
            .find(|session| session.session_id == "ses_bench_0001")
            .unwrap();
        let store = Store::open(store_path.clone()).unwrap();
        sync_session(&store, &Opencode, &session).unwrap();
        let turns = store
            .query_turns(&TurnQuery {
                session: Some(session.session_id),
                ..TurnQuery::default()
            })
            .unwrap();
        let projected = turns
            .iter()
            .map(|turn| {
                format!(
                    "{} {}",
                    turn["role"].as_str().unwrap_or_default(),
                    turn["said"].as_str().unwrap_or_default()
                )
            })
            .collect::<Vec<_>>();
        assert!(
            projected
                .iter()
                .any(|turn| turn == "assistant bench reply 5"),
            "turns: {projected:#?}"
        );
        assert!(
            projected.iter().any(|turn| {
                turn == "tool tool bash\ninput: {\"command\":\"cargo test bench_5\"}\noutput: \"tests passed\"\nerror: \"stderr empty\""
            }),
            "turns: {projected:#?}"
        );
        drop(store);
        let _ = std::fs::remove_file(transcript);
        let _ = std::fs::remove_file(store_path);
    }

    // FAIL-PRE-FIX: `reasoning`, `patch`, and every kind the match had no arm
    // for were dropped, so 48 of one real session's 55 assistant turns and all
    // 107 of its tool turns projected as empty bodies.
    #[test]
    fn every_content_bearing_part_kind_projects_a_body() {
        let transcript = temp_db("kinds-transcript");
        let store_path = temp_db("kinds-store");
        std::fs::copy("tests/fixtures/opencode/bench/opencode.db", &transcript).unwrap();
        let connection = rusqlite::Connection::open(&transcript).unwrap();
        let message: String = connection
            .query_row(
                "SELECT id FROM message WHERE session_id = 'ses_bench_0001' ORDER BY id LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        for (id, data) in [
            (
                "part-reasoning",
                r#"{"type":"reasoning","text":"weighing the two shapes"}"#,
            ),
            (
                "part-patch",
                r#"{"type":"patch","hash":"abc1234","files":["crates/boop/src/mail.rs"]}"#,
            ),
            ("part-step", r#"{"type":"step-start","snapshot":"abc1234"}"#),
            (
                "part-future",
                r#"{"type":"a-kind-from-the-future","payload":"keep me"}"#,
            ),
        ] {
            connection
                .execute(
                    "INSERT INTO part (id, message_id, data) VALUES (?1, ?2, ?3)",
                    rusqlite::params![id, message, data],
                )
                .unwrap();
        }
        drop(connection);

        let session = sessions_from(&transcript)
            .unwrap()
            .into_iter()
            .find(|session| session.session_id == "ses_bench_0001")
            .unwrap();
        let store = Store::open(store_path.clone()).unwrap();
        sync_session(&store, &Opencode, &session).unwrap();
        let turns = store
            .query_turns(&TurnQuery {
                session: Some(session.session_id),
                ..TurnQuery::default()
            })
            .unwrap();
        let bodies: Vec<String> = turns
            .iter()
            .map(|turn| turn["said"].as_str().unwrap_or_default().to_owned())
            .collect();
        assert!(
            bodies
                .iter()
                .any(|body| body == "reasoning: weighing the two shapes"),
            "bodies: {bodies:#?}"
        );
        assert!(
            bodies
                .iter()
                .any(|body| body == "patch abc1234\nfiles: crates/boop/src/mail.rs"),
            "bodies: {bodies:#?}"
        );
        assert!(
            bodies.iter().any(
                |body| body.starts_with("a-kind-from-the-future (unprojected)")
                    && body.contains("keep me")
            ),
            "an unknown kind keeps its raw event: {bodies:#?}"
        );
        assert!(
            !bodies.iter().any(|body| body.contains("step-start")),
            "a structural part writes no turn: {bodies:#?}"
        );
        drop(store);
        let _ = std::fs::remove_file(transcript);
        let _ = std::fs::remove_file(store_path);
    }

    /// RECEIPT (7.4). No turn this projection writes has an empty body: an
    /// empty chat row can only come from an event that was empty itself.
    #[test]
    fn no_projected_turn_from_the_fixture_has_an_empty_body() {
        let transcript = temp_db("nonempty-transcript");
        let store_path = temp_db("nonempty-store");
        std::fs::copy("tests/fixtures/opencode/bench/opencode.db", &transcript).unwrap();
        let session = sessions_from(&transcript)
            .unwrap()
            .into_iter()
            .find(|session| session.session_id == "ses_bench_0001")
            .unwrap();
        let store = Store::open(store_path.clone()).unwrap();
        sync_session(&store, &Opencode, &session).unwrap();
        let turns = store
            .query_turns(&TurnQuery {
                session: Some(session.session_id),
                ..TurnQuery::default()
            })
            .unwrap();
        assert!(!turns.is_empty(), "the fixture projects turns");
        let empty: Vec<&serde_json::Value> = turns
            .iter()
            .filter(|turn| turn["said"].as_str().unwrap_or_default().is_empty())
            .collect();
        assert!(empty.is_empty(), "empty projected bodies: {empty:#?}");
        drop(store);
        let _ = std::fs::remove_file(transcript);
        let _ = std::fs::remove_file(store_path);
    }

    /// One scratch database path per test, per process.
    fn temp_db(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "boop-opencode-{name}-{}-{:?}.db",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn opencode_parts_fixture_receipt_uses_one_part_query_and_preserves_order() {
        let connection = super::open_read_only(std::path::Path::new(
            "tests/fixtures/opencode/bench/opencode.db",
        ))
        .unwrap();
        let messages = messages_after(&connection, "ses_bench_0001", 0).unwrap();
        PART_SELECTS.store(0, Ordering::Relaxed);
        connection.trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(count_part_selects));
        let mut parts: Vec<Vec<Part>> = (0..messages.len()).map(|_| Vec::new()).collect();
        visit_parts_for_messages(&connection, &messages, |message_index, part| {
            parts[message_index].push(part);
            Ok(())
        })
        .unwrap();
        connection.trace_v2(TraceEventCodes::empty(), None);

        let part_count: usize = parts.iter().map(Vec::len).sum();
        assert_eq!(messages.len(), 300);
        assert_eq!(parts.len(), 300);
        assert_eq!(part_count, 340);
        assert_eq!(PART_SELECTS.load(Ordering::Relaxed), 1);
        let tail = messages_after(&connection, "ses_bench_0001", messages[99].rowid).unwrap();
        assert_eq!(tail.len(), 200);
        assert_eq!(
            tail.first().map(|message| message.rowid),
            Some(messages[100].rowid)
        );
        assert_eq!(
            tail.last().map(|message| message.rowid),
            Some(messages[299].rowid)
        );

        let first = &parts[0];
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].kind, "text");
        assert_eq!(first[0].text, "bench prompt 0");
        assert!(first[0].input.is_none());

        let tool_message = &parts[5];
        assert_eq!(
            tool_message
                .iter()
                .map(|part| part.kind.as_str())
                .collect::<Vec<_>>(),
            ["text", "tool"]
        );
        assert_eq!(tool_message[1].tool, "bash");
        assert_eq!(
            tool_message[1]
                .input
                .as_ref()
                .and_then(|input| input.get("command"))
                .and_then(|command| command.as_str()),
            Some("cargo test bench_5")
        );
    }

    #[test]
    fn opencode_parts_batch_skips_malformed_json_without_reordering() {
        let path =
            std::env::temp_dir().join(format!("boop-opencode-malformed-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::fs::copy("tests/fixtures/opencode/bench/opencode.db", &path).unwrap();
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute(
                "INSERT INTO part(id, message_id, data) VALUES (?1, ?2, ?3)",
                rusqlite::params![
                    "ses_bench_0001_msg_0005_p9",
                    "ses_bench_0001_msg_0005",
                    "{malformed"
                ],
            )
            .unwrap();

        let connection = super::open_read_only(&path).unwrap();
        let messages = messages_after(&connection, "ses_bench_0001", 0).unwrap();
        let mut parts: Vec<Vec<Part>> = (0..messages.len()).map(|_| Vec::new()).collect();
        visit_parts_for_messages(&connection, &messages, |message_index, part| {
            parts[message_index].push(part);
            Ok(())
        })
        .unwrap();

        assert_eq!(parts.iter().map(Vec::len).sum::<usize>(), 340);
        assert_eq!(
            parts[5]
                .iter()
                .map(|part| part.kind.as_str())
                .collect::<Vec<_>>(),
            ["text", "tool"]
        );
        let _ = std::fs::remove_file(path);
    }

    // ---- facet 3 ----

    struct TmuxGuard {
        socket: String,
    }

    static NEXT_SOCKET: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    impl TmuxGuard {
        fn new() -> TmuxGuard {
            let socket = format!(
                "boop-test-{}-oc{}",
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

    fn spec(guard: &TmuxGuard) -> SpawnSpec {
        SpawnSpec {
            effort: None,
            harness: HarnessId::Opencode,
            branch: "lane-test".to_owned(),
            base_sha: "0000000000000000000000000000000000000000".to_owned(),
            main_tree: true,
            setup: Vec::new(),
            prompt: "/tmp/brief.md".to_owned(),
            resume_session: None,
            socket: Some(guard.socket.clone()),
            worktree_dir: None,
            repo: std::env::temp_dir(),
            env_stamp: None,
            model: Some("m".to_owned()),
            variant: None,
            bin: None,
            on_exit: None,
            tmux: None,
            lane: "lane-test".to_owned(),
            mail_dir: std::env::temp_dir(),
            warm_start: false,
        }
    }

    #[test]
    fn a_missing_model_refuses_to_build_a_command() {
        let guard = TmuxGuard::new();
        let mut req = spec(&guard);
        req.model = None;
        let error = launch_command(&req).unwrap_err();
        assert!(error.to_string().contains("no model"));
    }

    #[test]
    fn launch_command_cats_the_prompt_path_under_the_resolved_model() {
        let guard = TmuxGuard::new();
        let mut req = spec(&guard);
        req.model = Some("openrouter/deepseek/deepseek-v4-flash-0731".to_owned());
        let command = launch_command(&req).unwrap();
        assert!(command.contains("opencode run -m 'openrouter/deepseek/deepseek-v4-flash-0731'"));
        assert!(command.contains("--auto \"$(cat \"/tmp/brief.md\")\""));
    }

    #[test]
    fn variant_flag_is_emitted_when_set() {
        let guard = TmuxGuard::new();
        let mut req = spec(&guard);
        req.variant = Some("low".to_owned());
        let command = launch_command(&req).unwrap();
        assert!(command.contains(" --variant 'low'"), "{command}");
    }

    #[test]
    fn no_variant_means_no_flag_and_byte_identical() {
        let guard = TmuxGuard::new();
        let req = spec(&guard);
        let command = launch_command(&req).unwrap();
        assert!(!command.contains("--variant"), "{command}");
        assert_eq!(
            command,
            "opencode run -m 'm' --auto \"$(cat \"/tmp/brief.md\")\""
        );
    }

    #[test]
    fn opencode_launch_resumes_with_session_id() {
        let guard = TmuxGuard::new();
        let mut req = spec(&guard);
        req.resume_session = Some("ses_abc123".to_owned());
        let command = launch_command(&req).unwrap();
        assert!(command.contains("-s 'ses_abc123'"));
    }

    /// The epilogue lands after the harness command and the lane re-raises
    /// the harness exit code.
    #[test]
    fn on_exit_appends_and_reraises_the_exit_code() {
        let guard = TmuxGuard::new();
        let mut req = spec(&guard);
        req.on_exit =
            Some("boop hail --to 'coord' --kind result --body \"lane done rc=$__rc\"".to_owned());
        let command = launch_command(&req).unwrap();
        assert!(command.contains("; __rc=$?; boop hail --to 'coord'"));
        assert!(command.ends_with("; exit $__rc"));
    }

    #[test]
    fn opencode_spawn_returns_handle_and_stop_tears_down() {
        let guard = TmuxGuard::new();
        let repo = TempRepo::new();
        let worktree = repo.worktree.clone();
        let mut req = spec(&guard);
        req.main_tree = false;
        req.base_sha = repo.sha.clone();
        req.repo = repo.dir.clone();
        req.worktree_dir = Some(worktree.clone());
        let opencode = Opencode;
        let session = opencode.spawn(&req).unwrap();
        assert_eq!(
            session
                .tmux
                .as_deref()
                .map(|t| t.starts_with("boop-agent-")),
            Some(true)
        );
        assert_eq!(session.tmux_socket.as_deref(), Some(guard.socket.as_str()));
        assert!(
            worktree.join("seed.txt").exists(),
            "worktree must be created by spawn"
        );
        opencode.stop(&session).unwrap();
        assert!(!has_session_on(&guard, session.tmux.as_deref().unwrap()));
    }

    fn has_session_on(guard: &TmuxGuard, name: &str) -> bool {
        boop_store::tmux::mux()
            .has_session(Some(&guard.socket), name)
            .unwrap_or(false)
    }
}
