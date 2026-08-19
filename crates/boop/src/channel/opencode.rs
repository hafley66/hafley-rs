//! The opencode lane channel: one `opencode acp` child per conversation. The
//! `opencode run` + store-scrape path is gone; the store readers below stay
//! because `channel/tui.rs` still calls them.

use std::path::Path;
use std::process::Child;

use anyhow::Result;
use tracing::{debug, warn};

use crate::channel::acp::AcpChannel;
use crate::channel::ChannelSpec;

pub struct OpencodeChannel;

impl OpencodeChannel {
    /// Open the opencode conversation over ACP.
    pub fn open(spec: &ChannelSpec) -> Result<AcpChannel> {
        AcpChannel::open(spec, &["opencode".to_owned(), "acp".to_owned()])
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
