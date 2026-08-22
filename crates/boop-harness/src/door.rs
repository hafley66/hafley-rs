//! The write half of a running session: put one message in front of a TUI the
//! user owns, and learn when that TUI next ends a turn. One door per harness,
//! stateless, opening its transport per call.

use std::time::Duration;

use anyhow::Result;

use crate::live::{now_ms, LiveSession, LiveSessions};

pub mod claude;
pub mod codex;
pub mod kimi;
pub mod opencode;

/// What became of one delivery.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Delivered {
    /// The session took the text into its current turn.
    Injected,
    /// The text is queued and the session reads it at its next turn boundary.
    QueuedForTurnBoundary,
    /// No door answered; the string says which check failed.
    Unreachable(String),
}

/// The moment a session finished a turn with nothing left queued.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct IdleNotice {
    pub at_ms: u64,
    pub status_line: Option<String>,
}

impl IdleNotice {
    pub fn now(status_line: Option<String>) -> Self {
        IdleNotice {
            at_ms: now_ms(),
            status_line,
        }
    }
}

/// The control plane of one harness.
pub trait Door: Send + Sync {
    /// Write `body` to `session`. Transport failure is `Unreachable`, not an
    /// `Err`; an `Err` means the request could not be formed at all.
    fn deliver(&self, session: &LiveSession, body: &str) -> Result<Delivered>;

    /// Resolve once the session next ends a turn with nothing queued.
    fn notify_idle(&self, session: &LiveSession, timeout: Duration) -> Result<IdleNotice>;
}

/// The harness with no control plane: nothing is running that boop can find,
/// and nothing written reaches a session. Every default trait body answers
/// from this one value, so an adapter that declares no door is explicit.
pub struct Unreachable;

/// The shared value `Harness::live` and `Harness::door` hand back by default.
pub static UNREACHABLE: Unreachable = Unreachable;

impl LiveSessions for Unreachable {
    fn live_sessions(&self) -> Result<Vec<LiveSession>> {
        Ok(Vec::new())
    }
}

impl Door for Unreachable {
    fn deliver(&self, session: &LiveSession, _body: &str) -> Result<Delivered> {
        Ok(Delivered::Unreachable(format!(
            "harness `{}` declares no door",
            session.harness
        )))
    }

    fn notify_idle(&self, session: &LiveSession, _timeout: Duration) -> Result<IdleNotice> {
        anyhow::bail!("harness `{}` reports no idle signal", session.harness)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::{DoorAddress, LiveStatus};

    pub(crate) fn probe(harness: crate::harness::HarnessId) -> LiveSession {
        LiveSession {
            harness,
            session_id: "probe".into(),
            pid: None,
            cwd: None,
            tmux_pane: None,
            status: LiveStatus::Unknown,
            door: DoorAddress::None,
            observed_ms: 0,
        }
    }

    /// RECEIPT. The default door answers without a panic and without an Err,
    /// so a caller reads one `Delivered` shape from every harness.
    #[test]
    fn the_default_door_reports_itself_unreachable() {
        let session = probe(crate::harness::HarnessId::Kimi);
        assert_eq!(
            UNREACHABLE.deliver(&session, "hello").unwrap(),
            Delivered::Unreachable("harness `kimi` declares no door".into())
        );
        assert!(UNREACHABLE.live_sessions().unwrap().is_empty());
        assert!(UNREACHABLE
            .notify_idle(&session, Duration::from_millis(1))
            .is_err());
    }
}
