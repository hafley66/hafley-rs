//! One live conversation per lane, driven the same way whatever the harness.
//! `Harness::open_channel` mints one; `crate::supervise` is the only caller.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Result;

pub mod claude;
pub mod codex;
pub mod jsonrpc;
pub mod kimi;
pub mod opencode;
pub mod tui;

/// Where a delivered message landed relative to the turn that was running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// Accepted into the turn already in flight.
    MidTurn,
    /// Held by the supervisor; a resume turn opens when the running one ends.
    NextTurn,
}

impl Delivery {
    pub fn as_str(self) -> &'static str {
        match self {
            Delivery::MidTurn => "midturn",
            Delivery::NextTurn => "nextturn",
        }
    }
}

/// The layer named by raw failure evidence. Classification is independent of
/// retry policy: an unknown watchdog stall may still receive a bounded retry,
/// while a harness configuration error does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureClass {
    Unknown,
    Transport,
    Harness,
    Provider,
}

impl FailureClass {
    pub fn as_str(self) -> &'static str {
        match self {
            FailureClass::Unknown => "unknown",
            FailureClass::Transport => "transport",
            FailureClass::Harness => "harness",
            FailureClass::Provider => "provider",
        }
    }
}

/// Raw fields cited by a failure classification. The ordered map gives logs,
/// mailbox bodies, and pane artifacts one deterministic rendering.
pub type RawEvidence = BTreeMap<String, String>;

/// One turn-lifecycle event from a live conversation. A channel emits these;
/// the supervisor and the concatmap feed subscribe to them.
#[derive(Clone, Debug)]
pub enum TurnEvent {
    /// The model began its reply (the harness painted/streamed after submit).
    /// TUI-backed channels emit this; subprocess-backed channels go straight
    /// from running to an end verdict.
    Started,
    /// The turn completed cleanly.
    Done { detail: String },
    /// The turn failed hard (not retryable).
    Failed {
        detail: String,
        classification: FailureClass,
        evidence: RawEvidence,
    },
    /// The turn failed before the agent could handle it; bounded retry allowed.
    Retryable {
        detail: String,
        classification: FailureClass,
        evidence: RawEvidence,
    },
}

impl TurnEvent {
    pub fn ok(detail: impl Into<String>) -> TurnEvent {
        TurnEvent::Done {
            detail: detail.into(),
        }
    }

    pub fn failed(detail: impl Into<String>) -> TurnEvent {
        TurnEvent::Failed {
            detail: detail.into(),
            classification: FailureClass::Harness,
            evidence: RawEvidence::new(),
        }
    }

    /// Compatibility constructor for test channels without richer evidence.
    /// Its classification is explicitly unknown, never provider.
    pub fn flaked(detail: impl Into<String>) -> TurnEvent {
        TurnEvent::Retryable {
            detail: detail.into(),
            classification: FailureClass::Unknown,
            evidence: RawEvidence::new(),
        }
    }

    pub fn failure(
        detail: impl Into<String>,
        classification: FailureClass,
        evidence: RawEvidence,
    ) -> TurnEvent {
        TurnEvent::Failed {
            detail: detail.into(),
            classification,
            evidence,
        }
    }

    pub fn retryable_failure(
        detail: impl Into<String>,
        classification: FailureClass,
        evidence: RawEvidence,
    ) -> TurnEvent {
        TurnEvent::Retryable {
            detail: detail.into(),
            classification,
            evidence,
        }
    }

    /// The one-line reason, printed by the supervisor and never parsed.
    pub fn detail(&self) -> &str {
        match self {
            TurnEvent::Started => "started",
            TurnEvent::Done { detail }
            | TurnEvent::Failed { detail, .. }
            | TurnEvent::Retryable { detail, .. } => detail,
        }
    }

    /// True only for a clean completion.
    pub fn is_done(&self) -> bool {
        matches!(self, TurnEvent::Done { .. })
    }

    /// True only when the channel/supervisor granted a bounded retry.
    pub fn retryable(&self) -> bool {
        matches!(self, TurnEvent::Retryable { .. })
    }

    pub fn classification(&self) -> Option<FailureClass> {
        match self {
            TurnEvent::Failed { classification, .. }
            | TurnEvent::Retryable { classification, .. } => Some(*classification),
            TurnEvent::Started | TurnEvent::Done { .. } => None,
        }
    }

    pub fn evidence(&self) -> &RawEvidence {
        match self {
            TurnEvent::Failed { evidence, .. } | TurnEvent::Retryable { evidence, .. } => evidence,
            TurnEvent::Started | TurnEvent::Done { .. } => {
                static EMPTY: std::sync::LazyLock<RawEvidence> =
                    std::sync::LazyLock::new(RawEvidence::new);
                &EMPTY
            }
        }
    }
}

/// What one lane conversation needs before its first turn.
#[derive(Clone, Debug)]
pub struct ChannelSpec {
    /// The harness's own model spelling; `None` takes the harness default.
    pub model: Option<String>,
    pub cwd: PathBuf,
    /// An existing conversation to continue instead of starting a new one.
    pub resume: Option<String>,
    /// The lane whose trail the harness child's stderr is written to. `None`
    /// is a caller outside a lane; its child inherits the pane's stderr.
    pub lane: Option<String>,
}

/// One live conversation. Every harness answers the same four calls, so the
/// supervisor holds no harness id and no per-harness branch.
pub trait LaneChannel: Send {
    /// The harness's own id for this conversation, once it exists. Written to
    /// the lane's registry route so a later resume can find it.
    fn conversation_id(&self) -> Option<String>;

    /// The namespace of `conversation_id`; a TUI may temporarily expose its
    /// transport target while a harness session id is not yet resolved.
    fn conversation_id_kind(&self) -> &'static str {
        "harness_session"
    }

    /// Pane carrying the harness itself, when the channel owns one.
    fn tmux_target(&self) -> Option<&str> {
        None
    }

    fn tmux_socket(&self) -> Option<&str> {
        None
    }

    /// Strongest raw state still available after a turn or watchdog event.
    fn raw_evidence(&self) -> RawEvidence {
        RawEvidence::new()
    }

    /// Hand the lane's brief over before the first turn. A transport that can
    /// lose its conversation re-feeds this text after a respawn; a harness that
    /// keeps its own history ignores it.
    fn set_brief(&mut self, _brief: &str) {}

    /// Send `text` as a new turn. Called once to open the lane with the brief,
    /// then again for every batch of messages that arrived between turns.
    fn start_turn(&mut self, text: &str) -> Result<()>;

    /// Offer `text` to the turn already in flight. `NextTurn` means the harness
    /// took nothing and the supervisor must re-offer it after `join`.
    fn steer(&mut self, text: &str) -> Result<Delivery>;

    /// Block up to `timeout` for the next turn event. `None` means the turn is
    /// still running, which is when the supervisor offers it new text.
    fn next_event(&mut self, timeout: std::time::Duration) -> Result<Option<TurnEvent>>;

    /// Clear a turn that will never complete (a message queued but never run)
    /// so a retry starts from an idle harness; no-op without a clear key.
    fn interrupt(&mut self) -> Result<()> {
        Ok(())
    }

    /// Epoch millis of the newest harness-side write for this conversation.
    /// `None` means no signal; the supervisor then measures from turn start.
    fn last_activity_ms(&self) -> Option<u64> {
        None
    }

    /// Release the harness child.
    fn close(&mut self) -> Result<()>;
}

/// Milliseconds since the epoch.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_spells_both_tiers() {
        assert_eq!(Delivery::MidTurn.as_str(), "midturn");
        assert_eq!(Delivery::NextTurn.as_str(), "nextturn");
    }

    #[test]
    fn turn_event_carries_its_verdict() {
        assert!(TurnEvent::ok("done").is_done());
        assert!(!TurnEvent::failed("boom").is_done());
        assert!(TurnEvent::flaked("flake").retryable());
        assert!(!TurnEvent::failed("boom").retryable());
        assert_eq!(TurnEvent::ok("done").detail(), "done");
    }
}
