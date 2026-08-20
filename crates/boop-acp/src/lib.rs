//! The typed channel every lane conversation runs over: an ACP client on a
//! stdio child for the harnesses that speak it, a tmux TUI driver for the ones
//! that do not, and the one `LaneChannel` trait the supervisor sees.

pub mod agents;
pub mod channel;
pub mod host;

pub use channel::acp::PromptQueueing;
pub use channel::{now_ms, ChannelSpec, Delivery, LaneChannel, TurnEvent};
