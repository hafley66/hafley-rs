//! Process control: what a lane is, the worktree and warm start it spawns
//! into, the supervisor that owns the harness child and writes its result row,
//! the mailbox a lane drains, and the coroutine host a caller embeds.

pub mod concatmap;
pub mod config;
pub mod deliver;
pub mod host;
pub mod inbox;
pub mod lane;
pub mod mailwait;
pub mod supervise;

pub use lane::{Effort, LaneIdentity, ModelSpec};
pub use supervise::ParentDeathPolicy;
