//! One live conversation per lane, driven the same way whatever the harness.
//! `Harness::open_channel` mints one; boop-proc's supervisor is the only caller.

use std::path::PathBuf;

use anyhow::Result;

pub mod acp;
pub mod claude;
pub mod codex;
pub mod jsonrpc;
pub mod terminal;

/// Where a delivered message landed relative to the turn that was running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// Accepted into the turn already in flight.
    MidTurn,
    /// Held by the supervisor; a resume turn opens when the running one ends.
    NextTurn,
}

/// Observable output from one completed turn. Harness adapters populate this
/// when their protocol exposes assistant text and tool calls.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TurnReceipt {
    pub text: String,
    pub tool_calls: u32,
}

impl Delivery {
    pub fn as_str(self) -> &'static str {
        match self {
            Delivery::MidTurn => "midturn",
            Delivery::NextTurn => "nextturn",
        }
    }
}

/// One turn-lifecycle event from a live conversation. A channel emits these;
/// the supervisor and the concatmap feed subscribe to them.
#[derive(Clone, Debug)]
pub enum TurnEvent {
    /// The model began its reply (the harness painted/streamed after submit).
    /// TUI-backed channels emit this; subprocess-backed channels go straight
    /// from running to an end verdict.
    Started,
    /// The turn completed cleanly.
    Done {
        detail: String,
        receipt: Option<TurnReceipt>,
    },
    /// The turn failed hard (not retryable).
    Failed { detail: String },
    /// The turn died on a provider flake the agent never saw; retryable.
    Flaked { detail: String },
}

impl TurnEvent {
    pub fn ok(detail: impl Into<String>) -> TurnEvent {
        TurnEvent::Done {
            detail: detail.into(),
            receipt: None,
        }
    }

    pub fn ok_with_receipt(detail: impl Into<String>, receipt: TurnReceipt) -> TurnEvent {
        TurnEvent::Done {
            detail: detail.into(),
            receipt: Some(receipt),
        }
    }

    pub fn failed(detail: impl Into<String>) -> TurnEvent {
        TurnEvent::Failed {
            detail: detail.into(),
        }
    }

    pub fn flaked(detail: impl Into<String>) -> TurnEvent {
        TurnEvent::Flaked {
            detail: detail.into(),
        }
    }

    /// The one-line reason, printed by the supervisor and never parsed.
    pub fn detail(&self) -> &str {
        match self {
            TurnEvent::Started => "started",
            TurnEvent::Done { detail, .. }
            | TurnEvent::Failed { detail }
            | TurnEvent::Flaked { detail } => detail,
        }
    }

    /// True only for a clean completion.
    pub fn is_done(&self) -> bool {
        matches!(self, TurnEvent::Done { .. })
    }

    /// True only for a provider flake the agent never saw.
    pub fn retryable(&self) -> bool {
        matches!(self, TurnEvent::Flaked { .. })
    }

    pub fn receipt(&self) -> Option<&TurnReceipt> {
        match self {
            TurnEvent::Done { receipt, .. } => receipt.as_ref(),
            _ => None,
        }
    }
}

/// What one lane conversation needs before its first turn.
#[derive(Clone, Debug)]
pub struct ChannelSpec {
    /// The harness's own model spelling; `None` takes the harness default.
    pub model: Option<String>,
    /// Reasoning effort, set as the agent's own session option. `None` leaves
    /// the agent's default; it never rides inside `model`.
    pub effort: Option<String>,
    pub cwd: PathBuf,
    /// An existing conversation to continue instead of starting a new one.
    pub resume: Option<String>,
    /// The lane whose trail the harness child's stderr is written to. `None`
    /// is a caller outside a lane; its child inherits the pane's stderr.
    pub lane: Option<String>,
    /// The executable the harness runs as, replacing the binary it names for
    /// itself (`ccz` is claude under the z.ai env). `None` is the harness's
    /// own binary.
    pub executable: Option<String>,
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

    #[test]
    fn turn_event_carries_an_optional_output_receipt() {
        let event = TurnEvent::ok_with_receipt(
            "done",
            TurnReceipt {
                text: "boop".into(),
                tool_calls: 0,
            },
        );
        assert_eq!(event.receipt().unwrap().text, "boop");
        assert_eq!(event.receipt().unwrap().tool_calls, 0);
        assert!(TurnEvent::ok("done").receipt().is_none());
    }
}
