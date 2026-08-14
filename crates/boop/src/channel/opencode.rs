//! The opencode lane channel: one `opencode run` child per turn. That child
//! binds no control port, so steer text lands on the next turn.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use crate::channel::{ChannelSpec, Delivery, LaneChannel, TurnEnd};

pub struct OpencodeChannel {
    cwd: PathBuf,
    model: Option<String>,
    session: Option<String>,
    turn: Option<Child>,
    /// Epoch millis the current turn started; the session-id lookup only
    /// accepts an opencode session created at or after it.
    turn_started_ms: u64,
}

impl OpencodeChannel {
    pub fn open(spec: &ChannelSpec) -> Result<OpencodeChannel> {
        Ok(OpencodeChannel {
            cwd: spec.cwd.clone(),
            model: spec.model.clone(),
            session: spec.resume.clone(),
            turn: None,
            turn_started_ms: 0,
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
                .spawn()
                .context("spawn opencode run")?,
        );
        Ok(())
    }

    fn steer(&mut self, _text: &str) -> Result<Delivery> {
        Ok(Delivery::NextTurn)
    }

    fn poll_turn(&mut self, timeout: std::time::Duration) -> Result<Option<TurnEnd>> {
        let Some(turn) = self.turn.as_mut() else {
            return Ok(Some(TurnEnd::failed("no opencode turn to join")));
        };
        let Some(status) = wait_for(turn, timeout).context("wait opencode run")? else {
            return Ok(None);
        };
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
        let state = self.session.as_deref().and_then(last_message_state);
        if let Some(state) = &state {
            info!(
                conversation_id = self.session.as_deref().unwrap_or_default(),
                last_opencode_finish = state.finish.as_deref().unwrap_or_default(),
                last_opencode_error = state.error.as_deref().unwrap_or_default(),
                "opencode trailing message state"
            );
        }
        Ok(Some(match status {
            // `opencode run` exits 0 when the provider drops the stream; the
            // db's trailing MessageAbortedError is the only tell.
            0 => match state.as_ref().map(LastMessageState::aborted) {
                Some(true) => TurnEnd::flaked("rc=0 with an aborted stream"),
                _ => TurnEnd::ok("rc=0"),
            },
            other => TurnEnd::failed(format!("rc={other}")),
        }))
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
               SELECT max(time_created) FROM part WHERE session_id = ?1)",
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

/// The finish/error fields on OpenCode's newest message row. A missing finish
/// or an error means the stream was aborted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LastMessageState {
    pub(crate) finish: Option<String>,
    pub(crate) error: Option<String>,
}

impl LastMessageState {
    pub(crate) fn aborted(&self) -> bool {
        self.finish.is_none() || self.error.is_some()
    }

    pub(crate) fn completed(&self) -> bool {
        self.finish.as_deref() == Some("stop") && self.error.is_none()
    }
}

/// The newest message state for a conversation. OpenCode owns this database;
/// lookup failures leave the existing process exit-code behavior unchanged.
pub(crate) fn last_message_state(session: &str) -> Option<LastMessageState> {
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
    connection
        .query_row(
            "SELECT json_extract(data, '$.finish'), json_extract(data, '$.error.name')
              FROM message
              WHERE session_id = ?1
              ORDER BY time_created DESC LIMIT 1",
            rusqlite::params![session],
            |row| {
                Ok(LastMessageState {
                    finish: row.get(0)?,
                    error: row.get(1)?,
                })
            },
        )
        .map_err(|error| {
            debug!(conversation_id = session, error = %error, "opencode trailing message lookup returned no row");
            error
        })
        .ok()
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
    let directory = cwd.display().to_string();
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

    fn spec() -> ChannelSpec {
        ChannelSpec {
            model: Some("openrouter/deepseek/deepseek-v4-flash-0731".to_owned()),
            cwd: std::env::temp_dir(),
            resume: None,
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
            .poll_turn(std::time::Duration::from_millis(10))
            .unwrap()
            .unwrap();
        assert!(!end.ok);
    }

    #[test]
    fn a_resumed_channel_reports_its_conversation_before_the_first_turn() {
        let mut request = spec();
        request.resume = Some("ses_abc".to_owned());
        let channel = OpencodeChannel::open(&request).unwrap();
        assert_eq!(channel.conversation_id().as_deref(), Some("ses_abc"));
    }

    #[test]
    fn missing_finish_or_error_marks_a_trailing_message_aborted() {
        assert!(LastMessageState {
            finish: None,
            error: None,
        }
        .aborted());
        assert!(LastMessageState {
            finish: Some("stop".into()),
            error: Some("MessageAbortedError".into()),
        }
        .aborted());
        assert!(!LastMessageState {
            finish: Some("stop".into()),
            error: None,
        }
        .aborted());
    }

    #[test]
    fn only_a_clean_stop_is_terminal_success() {
        assert!(LastMessageState {
            finish: Some("stop".into()),
            error: None,
        }
        .completed());
        assert!(!LastMessageState {
            finish: Some("tool-calls".into()),
            error: None,
        }
        .completed());
        assert!(!LastMessageState {
            finish: None,
            error: Some("MessageAbortedError".into()),
        }
        .completed());
    }
}
