//! The opencode lane channel: one `opencode run` child per turn. That child
//! binds no control port, so steer text lands on the next turn.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use crate::channel::{ChannelSpec, Delivery, FailureClass, LaneChannel, RawEvidence, TurnEvent};

pub struct OpencodeChannel {
    cwd: PathBuf,
    model: Option<String>,
    session: Option<String>,
    turn: Option<Child>,
    /// Epoch millis the current turn started; the session-id lookup only
    /// accepts an opencode session created at or after it.
    turn_started_ms: u64,
    lane: Option<String>,
    last_status: Option<i32>,
    last_state: Option<LastMessageState>,
}

impl OpencodeChannel {
    pub fn open(spec: &ChannelSpec) -> Result<OpencodeChannel> {
        Ok(OpencodeChannel {
            cwd: spec.cwd.clone(),
            model: spec.model.clone(),
            session: spec.resume.clone(),
            turn: None,
            turn_started_ms: 0,
            lane: spec.lane.clone(),
            last_status: None,
            last_state: None,
        })
    }
}

impl LaneChannel for OpencodeChannel {
    fn conversation_id(&self) -> Option<String> {
        self.session.clone()
    }

    fn conversation_id_kind(&self) -> &'static str {
        "opencode_session"
    }

    fn raw_evidence(&self) -> RawEvidence {
        let mut evidence = self
            .last_state
            .as_ref()
            .map(LastMessageState::evidence)
            .unwrap_or_default();
        if let Some(status) = self.last_status {
            evidence.insert("process_exit_status".into(), status.to_string());
        }
        evidence
    }

    fn start_turn(&mut self, text: &str) -> Result<()> {
        if self.turn.is_some() {
            anyhow::bail!("an opencode turn is already running");
        }
        let mut command = Command::new("opencode");
        command.arg("run").arg("--auto");
        if let Some(model) = self.model.as_deref().filter(|value| !value.is_empty()) {
            command.args(["-m", model]);
        }
        if let Some(session) = &self.session {
            command.args(["-s", session]);
        }
        command.arg(text);
        self.turn_started_ms = crate::channel::now_ms();
        info!(
            cwd = %self.cwd.display(),
            model = self.model.as_deref().unwrap_or_default(),
            conversation_id = self.session.as_deref().unwrap_or_default(),
            text_bytes = text.len(),
            "opencode process turn starting"
        );
        self.turn = Some(
            command
                .current_dir(&self.cwd)
                .stdin(Stdio::null())
                .stderr(crate::trail::child_stderr(self.lane.as_deref()))
                .spawn()
                .context("spawn opencode run")?,
        );
        Ok(())
    }

    fn steer(&mut self, _text: &str) -> Result<Delivery> {
        Ok(Delivery::NextTurn)
    }

    fn next_event(&mut self, timeout: std::time::Duration) -> Result<Option<TurnEvent>> {
        let Some(turn) = self.turn.as_mut() else {
            return Ok(Some(TurnEvent::failed("no opencode turn to join")));
        };
        let Some(status) = wait_for(turn, timeout).context("wait opencode run")? else {
            return Ok(None);
        };
        self.last_status = Some(status);
        self.turn = None;
        if self.session.is_none() {
            self.session = newest_session(&self.cwd, self.turn_started_ms);
            match self.session.as_deref() {
                Some(conversation_id) => info!(
                    conversation_id,
                    conversation_id_kind = "opencode_session",
                    "opencode session resolved"
                ),
                None => {
                    warn!(cwd = %self.cwd.display(), since_ms = self.turn_started_ms, "opencode session lookup returned no session")
                }
            }
        }
        let state = self
            .session
            .as_deref()
            .and_then(|session| last_message_state(session, self.turn_started_ms));
        self.last_state = state.clone();
        if let Some(state) = &state {
            info!(
                conversation_id = self.session.as_deref().unwrap_or_default(),
                last_opencode_finish = state.finish.as_deref().unwrap_or("<missing>"),
                last_opencode_error = state.error.as_deref().unwrap_or("<missing>"),
                "opencode trailing message state"
            );
        }
        let mut evidence = state
            .as_ref()
            .map(LastMessageState::evidence)
            .unwrap_or_default();
        evidence.insert("process_exit_status".into(), status.to_string());
        Ok(Some(
            match (status, state.as_ref().and_then(LastMessageState::failure)) {
                (0, Some((classification, reason))) => {
                    TurnEvent::retryable_failure(reason, classification, evidence)
                }
                (0, None) if state.as_ref().is_some_and(LastMessageState::completed) => {
                    TurnEvent::ok("rc=0; opencode finish=stop")
                }
                (0, None) => TurnEvent::failure(
                    "opencode exited rc=0 without terminal assistant evidence",
                    FailureClass::Unknown,
                    evidence,
                ),
                (other, Some((classification, reason))) => TurnEvent::failure(
                    format!("{reason}; process exited rc={other}"),
                    classification,
                    evidence,
                ),
                (other, _) => TurnEvent::failure(
                    format!("opencode process exited rc={other}"),
                    FailureClass::Harness,
                    evidence,
                ),
            },
        ))
    }

    fn last_activity_ms(&self) -> Option<u64> {
        // The first turn's session id is only pinned at turn end, so a live
        // lookup keeps the stall watchdog from seeing a healthy turn as silent.
        let session = match self.session.clone() {
            Some(session) => session,
            None => newest_session(&self.cwd, self.turn_started_ms)?,
        };
        newest_activity(&session)
    }

    fn close(&mut self) -> Result<()> {
        if let Some(mut turn) = self.turn.take() {
            let _ = turn.kill();
            let _ = turn.wait();
        }
        Ok(())
    }
}

/// The newest message/part write for a session. A live turn streams part rows,
/// so a flat-lined value under a running child is a stalled provider stream.
pub(crate) fn newest_activity(session: &str) -> Option<u64> {
    let path = crate::harness::opencode::store_path()?;
    let connection = rusqlite::Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    connection
        .query_row(
            "SELECT max(newest) FROM (
               SELECT max(time_updated) AS newest FROM message WHERE session_id = ?1
               UNION ALL
               SELECT max(time_updated) FROM part WHERE session_id = ?1)",
            rusqlite::params![session],
            |row| row.get::<_, Option<i64>>(0),
        )
        .ok()
        .flatten()
        .map(|value| value as u64)
}

/// Reap `child` if it exits within `timeout`; `None` means still running.
pub(crate) fn wait_for(child: &mut Child, timeout: std::time::Duration) -> Result<Option<i32>> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status.code().unwrap_or(-1)));
        }
        if std::time::Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// OpenCode's newest assistant message plus a compact tail of its part-event
/// stream. User messages intentionally never enter this state: they do not
/// carry finish/error fields and were the source of the empty abort report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LastMessageState {
    pub(crate) message_id: String,
    pub(crate) role: String,
    pub(crate) finish: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) error_json: Option<String>,
    pub(crate) provider_id: Option<String>,
    pub(crate) model_id: Option<String>,
    pub(crate) created_ms: u64,
    pub(crate) updated_ms: u64,
    pub(crate) completed_ms: Option<u64>,
    pub(crate) part_events: Vec<String>,
}

impl LastMessageState {
    /// A missing finish and missing error is in-progress/unknown state. Only an
    /// explicit error object produces a failure classification.
    pub(crate) fn failure(&self) -> Option<(FailureClass, String)> {
        let error = self.error.as_deref()?;
        let lower = error.to_ascii_lowercase();
        let raw = self.error_json.as_deref().unwrap_or_default();
        let provider_cited = ["statusCode", "responseBody", "providerError", "upstream"]
            .iter()
            .any(|field| raw.contains(field));
        let classification = if provider_cited {
            FailureClass::Provider
        } else if ["abort", "stream", "connection", "timeout", "network"]
            .iter()
            .any(|word| lower.contains(word))
        {
            FailureClass::Transport
        } else {
            FailureClass::Harness
        };
        Some((classification, format!("opencode assistant error={error}")))
    }

    pub(crate) fn completed(&self) -> bool {
        self.finish.as_deref() == Some("stop") && self.error.is_none()
    }

    pub(crate) fn evidence(&self) -> RawEvidence {
        let mut evidence = RawEvidence::from([
            ("source".into(), "opencode.db message+part".into()),
            ("message_id".into(), self.message_id.clone()),
            ("message_role".into(), self.role.clone()),
            ("message_created_ms".into(), self.created_ms.to_string()),
            ("message_updated_ms".into(), self.updated_ms.to_string()),
            (
                "finish".into(),
                self.finish.clone().unwrap_or_else(|| "<missing>".into()),
            ),
            (
                "error".into(),
                self.error.clone().unwrap_or_else(|| "<missing>".into()),
            ),
        ]);
        if let Some(value) = self.error_json.as_ref() {
            evidence.insert("error_json".into(), value.clone());
        }
        if let Some(error) = self.error.as_deref() {
            let raw = self.error_json.as_deref().unwrap_or_default();
            let basis = ["statusCode", "responseBody", "providerError", "upstream"]
                .iter()
                .find(|field| raw.contains(**field))
                .map(|field| format!("error_json.{field}"))
                .or_else(|| {
                    let lower = error.to_ascii_lowercase();
                    ["abort", "stream", "connection", "timeout", "network"]
                        .iter()
                        .find(|word| lower.contains(**word))
                        .map(|word| format!("error.name contains {word}"))
                })
                .unwrap_or_else(|| "error.name".into());
            evidence.insert("classification_basis".into(), basis);
        }
        if let Some(value) = self.provider_id.as_ref() {
            evidence.insert("provider_id".into(), value.clone());
        }
        if let Some(value) = self.model_id.as_ref() {
            evidence.insert("model_id".into(), value.clone());
        }
        if let Some(value) = self.completed_ms {
            evidence.insert("message_completed_ms".into(), value.to_string());
        }
        for (index, event) in self.part_events.iter().enumerate() {
            evidence.insert(format!("part_event_{index}"), event.clone());
        }
        evidence
    }
}

/// The newest message state for a conversation. OpenCode owns this database;
/// lookup failures leave the existing process exit-code behavior unchanged.
pub(crate) fn last_message_state(session: &str, since_ms: u64) -> Option<LastMessageState> {
    let Some(path) = crate::harness::opencode::store_path() else {
        debug!(
            conversation_id = session,
            "opencode store path unavailable for trailing message lookup"
        );
        return None;
    };
    let connection = match rusqlite::Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(connection) => connection,
        Err(error) => {
            warn!(conversation_id = session, store_path = %path.display(), error = %error, "open opencode store for trailing message lookup failed");
            return None;
        }
    };
    last_message_state_from_connection(&connection, session, since_ms)
}

fn last_message_state_from_connection(
    connection: &rusqlite::Connection,
    session: &str,
    since_ms: u64,
) -> Option<LastMessageState> {
    let mut state = connection
        .query_row(
            "SELECT id,
                    json_extract(data, '$.role'),
                    json_extract(data, '$.finish'),
                    json_extract(data, '$.error.name'),
                    json_extract(data, '$.error'),
                    json_extract(data, '$.providerID'),
                    json_extract(data, '$.modelID'),
                    time_created,
                    time_updated,
                    json_extract(data, '$.time.completed')
              FROM message
              WHERE session_id = ?1
                AND json_extract(data, '$.role') = 'assistant'
                AND time_created >= ?2
              ORDER BY time_created DESC LIMIT 1",
            rusqlite::params![session, since_ms as i64],
            |row| {
                Ok(LastMessageState {
                    message_id: row.get(0)?,
                    role: row.get(1)?,
                    finish: row.get(2)?,
                    error: row.get(3)?,
                    error_json: row.get(4)?,
                    provider_id: row.get(5)?,
                    model_id: row.get(6)?,
                    created_ms: row.get::<_, i64>(7)? as u64,
                    updated_ms: row.get::<_, i64>(8)? as u64,
                    completed_ms: row.get::<_, Option<i64>>(9)?.map(|value| value as u64),
                    part_events: Vec::new(),
                })
            },
        )
        .map_err(|error| {
            debug!(conversation_id = session, error = %error, "opencode trailing message lookup returned no row");
            error
        })
        .ok()?;
    let mut statement = connection
        .prepare(
            "SELECT id, time_created, time_updated, data
               FROM part
              WHERE session_id = ?1 AND message_id = ?2
              ORDER BY time_updated DESC, id DESC LIMIT 8",
        )
        .ok()?;
    state.part_events = statement
        .query_map(rusqlite::params![session, state.message_id], |row| {
            let id: String = row.get(0)?;
            let created: i64 = row.get(1)?;
            let updated: i64 = row.get(2)?;
            let raw: String = row.get(3)?;
            let data: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
            let event_type = data.get("type").and_then(serde_json::Value::as_str).unwrap_or("-");
            let reason = data.get("reason").and_then(serde_json::Value::as_str).unwrap_or("-");
            let status = data
                .pointer("/state/status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("-");
            let error = data
                .pointer("/state/error")
                .or_else(|| data.get("error"))
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".into());
            Ok(format!(
                "id={id} type={event_type} status={status} reason={reason} error={error} created_ms={created} updated_ms={updated}"
            ))
        })
        .ok()?
        .filter_map(Result::ok)
        .collect();
    Some(state)
}

/// The newest opencode session under `cwd` created at or after `since_ms`.
/// opencode owns this store; boop only reads it.
pub(crate) fn newest_session(cwd: &Path, since_ms: u64) -> Option<String> {
    let path = crate::harness::opencode::store_path()?;
    let connection = rusqlite::Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| {
        warn!(cwd = %cwd.display(), error = %error, "open opencode store for session lookup failed");
        error
    })
    .ok()?;
    // opencode canonicalizes its directory (macOS /tmp -> /private/tmp), so
    // the query must compare the canonical spelling, not the caller's.
    let canonical = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_owned());
    let directory = canonical.display().to_string();
    connection
        .query_row(
            "SELECT id FROM session
              WHERE directory = ?1 AND time_created >= ?2
              ORDER BY time_created DESC LIMIT 1",
            rusqlite::params![directory, since_ms as i64],
            |row| row.get::<_, String>(0),
        )
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(finish: Option<&str>, error: Option<&str>) -> LastMessageState {
        LastMessageState {
            message_id: "msg-assistant".into(),
            role: "assistant".into(),
            finish: finish.map(str::to_owned),
            error: error.map(str::to_owned),
            error_json: error.map(|error| format!(r#"{{"name":"{error}"}}"#)),
            provider_id: Some("openrouter".into()),
            model_id: Some("provider/model".into()),
            created_ms: 10,
            updated_ms: 20,
            completed_ms: finish.map(|_| 20),
            part_events: vec!["id=part-1 type=step-start status=- reason=- error=-".into()],
        }
    }

    fn spec() -> ChannelSpec {
        ChannelSpec {
            model: Some("openrouter/deepseek/deepseek-v4-flash-0731".to_owned()),
            cwd: std::env::temp_dir(),
            resume: None,
            lane: None,
        }
    }

    #[test]
    fn steer_reports_the_next_turn_tier() {
        let mut channel = OpencodeChannel::open(&spec()).unwrap();
        assert_eq!(channel.steer("hello").unwrap(), Delivery::NextTurn);
    }

    #[test]
    fn polling_without_a_turn_is_a_failed_end_not_a_panic() {
        let mut channel = OpencodeChannel::open(&spec()).unwrap();
        let end = channel
            .next_event(std::time::Duration::from_millis(10))
            .unwrap()
            .unwrap();
        assert!(!end.is_done());
    }

    #[test]
    fn a_resumed_channel_reports_its_conversation_before_the_first_turn() {
        let mut request = spec();
        request.resume = Some("ses_abc".to_owned());
        let channel = OpencodeChannel::open(&request).unwrap();
        assert_eq!(channel.conversation_id().as_deref(), Some("ses_abc"));
    }

    #[test]
    fn empty_structured_fields_are_unknown_in_progress_state() {
        let empty = state(None, None);
        assert_eq!(empty.failure(), None);
        assert!(!empty.completed());
        assert_eq!(empty.evidence()["finish"], "<missing>");
        assert_eq!(empty.evidence()["error"], "<missing>");

        let aborted = state(None, Some("MessageAbortedError"));
        assert_eq!(
            aborted.failure().map(|failure| failure.0),
            Some(FailureClass::Transport)
        );

        let mut provider = state(None, Some("APIError"));
        provider.error_json = Some(r#"{"name":"APIError","statusCode":503}"#.into());
        assert_eq!(
            provider.failure().map(|failure| failure.0),
            Some(FailureClass::Provider),
            "provider classification cites a provider response field"
        );
        assert_eq!(
            provider.evidence()["classification_basis"],
            "error_json.statusCode"
        );
    }

    #[test]
    fn only_a_clean_stop_is_terminal_success() {
        assert!(state(Some("stop"), None).completed());
        assert!(!state(Some("tool-calls"), None).completed());
        assert!(!state(None, Some("MessageAbortedError")).completed());
    }

    #[test]
    fn trailing_state_selects_the_assistant_instead_of_a_newer_user_message() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE message (
                    id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER,
                    time_updated INTEGER, data TEXT
                 );
                 CREATE TABLE part (
                    id TEXT PRIMARY KEY, message_id TEXT, session_id TEXT,
                    time_created INTEGER, time_updated INTEGER, data TEXT
                 );",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO message VALUES (?1, 'ses-1', 10, 11, ?2)",
                rusqlite::params![
                    "msg-assistant",
                    r#"{"role":"assistant","providerID":"openrouter","modelID":"m","time":{"created":10}}"#
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO message VALUES (?1, 'ses-1', 20, 21, ?2)",
                rusqlite::params!["msg-user", r#"{"role":"user","time":{"created":20}}"#],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO part VALUES ('part-1', 'msg-assistant', 'ses-1', 12, 13, ?1)",
                [r#"{"type":"step-start"}"#],
            )
            .unwrap();

        let state = last_message_state_from_connection(&connection, "ses-1", 0).unwrap();
        assert_eq!(state.message_id, "msg-assistant");
        assert_eq!(state.role, "assistant");
        assert_eq!(state.finish, None);
        assert_eq!(state.error, None);
        assert_eq!(state.failure(), None);
        assert_eq!(state.part_events.len(), 1);
        assert_eq!(
            last_message_state_from_connection(&connection, "ses-1", 15),
            None,
            "an assistant from the previous turn cannot finish the new user turn"
        );
    }
}
