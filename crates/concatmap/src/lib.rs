//! The agent pipe over resident chats: read visible turns from the boop store,
//! drive a resident interactive opencode chat through tmux, accumulate outputs
//! as DL6 appends/retractions into one state file, and auto-commit that file.
//! The behavior of the pipe (which agent answers which request, with which
//! template, reminder cadence, retraction policy) is declared as hosted rules,
//! not shell code.
//!
//! DL6-ready, not DL6-dependent: v1 uses plain Rust structs whose shapes map
//! one-to-one onto DL6 relations (`fact.rs`, `rules.rs`). When the DL6 engine
//! lands, the same pipeline runs by replacing the internal fact/action
//! representation, no structural rework.

pub mod action;
pub mod cursor;
pub mod fact;
pub mod host;
pub mod interp;
pub mod pipe;
pub mod rules;
pub mod state;

pub use action::Action;
pub use fact::Fact;
pub use host::{Dl6Host, Host, Pair};
pub use rules::{load_rules, RuleSet};
pub use state::{fold_reply, State};
