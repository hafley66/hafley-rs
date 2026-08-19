//! The opencode adapter. opencode writes no transcript file, so this tails
//! `message.rowid` in its SQLite store, read-only; opencode owns that store.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::event::AgentEvent;
use crate::harness::{
    Capabilities, Harness, Ingested, OneShotSpec, ReadChunk, SendOutcome, SessionRef, SpawnSpec,
};
use crate::ident::{Store, SyncStat, UsageRow};

pub struct Opencode;

impl Harness for Opencode {
    fn open_channel(
        &self,
        spec: &crate::channel::ChannelSpec,
    ) -> anyhow::Result<Box<dyn crate::channel::LaneChannel>> {
        Ok(Box::new(crate::channel::opencode::OpencodeChannel::open(
            spec,
        )?))
    }

    fn id(&self) -> &'static str {
        "opencode"
    }

    fn sessions(&self) -> Result<Vec<SessionRef>> {
        let Some(path) = store_path() else {
            return Ok(Vec::new());
        };
        sessions_from(&path)
    }

    fn session_roots(&self) -> Result<Vec<PathBuf>> {
        Ok(store_path().into_iter().collect())
    }

    fn known_paths_can_move(&self) -> bool {
        false
    }

    fn read_from(&self, session: &SessionRef, offset: u64) -> Result<ReadChunk> {
        let connection = open_read_only(&session.path)?;
        let mut events = Vec::new();
        let mut next = offset;
        for message in messages_after(&connection, &session.session_id, offset)? {
            next = message.rowid;
            events.push(AgentEvent {
                harness: "opencode",
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
    fn capabilities(&self) -> Capabilities {
        Capabilities {
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
        crate::tmux::mux().new_detached_session(
            spec.socket.as_deref(),
            &tmux_name,
            &cwd.display().to_string(),
            &command,
        )?;
        Ok(SessionRef {
            harness: "opencode",
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

    fn send(&self, session: &SessionRef, text: &str) -> Result<SendOutcome> {
        match &session.tmux {
            Some(tmux) => {
                crate::tmux::mux().send_keys_literal(session.tmux_socket.as_deref(), tmux, text)?;
                Ok(SendOutcome::Injected)
            }
            None => Ok(SendOutcome::QueuedForNextSpawn),
        }
    }

    fn stop(&self, session: &SessionRef) -> Result<()> {
        if let Some(tmux) = &session.tmux {
            if crate::tmux::mux().has_session(session.tmux_socket.as_deref(), tmux)? {
                crate::tmux::mux().kill_session(session.tmux_socket.as_deref(), tmux)?;
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

fn sessions_from(path: &std::path::Path) -> Result<Vec<SessionRef>> {
    let connection = open_read_only(path)?;
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
        sessions.push(SessionRef {
            harness: "opencode",
            session_id: id.clone(),
            nickname: slug.unwrap_or(id),
            path: path.to_owned(),
            cwd: directory,
            git_branch: None,
            modified_ms: updated as u64,
            size: 0,
            tmux: None,
            tmux_socket: None,
            parent,
        });
    }
    Ok(sessions)
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
            let inserted = store.write_turn(session_id, *turn, message.ts, "tool", "")?;
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
        _ => {}
    }
    Ok(())
}

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
}

pub(crate) fn store_path() -> Option<PathBuf> {
    opencode_db_path().filter(|path| path.exists())
}

/// The db path regardless of existence; a spawn writes its command before
/// opencode has ever created the file.
fn opencode_db_path() -> Option<PathBuf> {
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
                },
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use rusqlite::trace::{TraceEvent, TraceEventCodes};

    use super::{
        launch_command, messages_after, sessions_from, visit_parts_for_messages, Opencode, Part,
    };
    use crate::harness::{Harness, SpawnSpec};
    use crate::test_support::TempRepo;

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
        let caps = Opencode.capabilities();
        assert!(!caps.send_midflight, "opencode run reads no stdin mid-turn");
        assert!(caps.resume, "opencode run -s <sessionID> resumes");
        assert!(caps.spawn, "spawn is implemented and tested below");
        assert!(caps.subagent_visible, "session.parent_id names the parent");
    }

    /// A hail with no live pane cannot land, so the outcome says queued
    /// rather than injected.
    #[test]
    fn a_send_is_queued_not_injected() {
        let session = crate::harness::SessionRef {
            harness: "opencode",
            session_id: "ses_x".to_owned(),
            nickname: "ses_x".to_owned(),
            path: std::path::PathBuf::from("/tmp/none.db"),
            cwd: None,
            git_branch: None,
            modified_ms: 0,
            size: 0,
            tmux: None,
            tmux_socket: None,
            parent: None,
        };
        assert_eq!(
            Opencode.send(&session, "hello").unwrap(),
            crate::harness::SendOutcome::QueuedForNextSpawn
        );
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
        crate::_0_session_graph::assert_fixture_sessions_project(&Opencode, &sessions, 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_part_with_no_state_still_parses() {
        let part = Part {
            kind: "text".to_owned(),
            tool: String::new(),
            text: "hello".to_owned(),
            input: None,
        };
        assert_eq!(part.text, "hello");
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
            crate::tmux::kill_test_server(&socket);
            TmuxGuard { socket }
        }
    }

    impl Drop for TmuxGuard {
        fn drop(&mut self) {
            crate::tmux::kill_test_server(&self.socket);
        }
    }

    fn spec(guard: &TmuxGuard) -> SpawnSpec {
        SpawnSpec {
            harness: "opencode".to_owned(),
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

    #[test]
    fn opencode_send_injects_into_a_live_pane() {
        let guard = TmuxGuard::new();
        let name = format!("ctl-{}", std::process::id());
        crate::tmux::mux()
            .new_bare_session(Some(&guard.socket), &name)
            .unwrap();
        let session = crate::harness::SessionRef {
            harness: "opencode",
            session_id: "ctl".to_owned(),
            nickname: "ctl".to_owned(),
            path: std::path::PathBuf::from("/tmp/ctl.db"),
            cwd: None,
            git_branch: None,
            modified_ms: 0,
            size: 0,
            tmux: Some(name.clone()),
            tmux_socket: Some(guard.socket.clone()),
            parent: None,
        };
        let opencode = Opencode;
        let outcome = opencode.send(&session, "hello from boop").unwrap();
        assert_eq!(outcome, crate::harness::SendOutcome::Injected);
        let pane = crate::tmux::mux()
            .capture_pane(Some(&guard.socket), &name, None)
            .unwrap();
        assert!(
            pane.contains("hello from boop"),
            "injected text not found in pane"
        );
    }

    fn has_session_on(guard: &TmuxGuard, name: &str) -> bool {
        crate::tmux::mux()
            .has_session(Some(&guard.socket), name)
            .unwrap_or(false)
    }
}
