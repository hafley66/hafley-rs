//! Process control: what a lane is, the worktree and warm start it spawns
//! into, the supervisor that owns the harness child and writes its result row,
//! the mailbox a lane drains, and the coroutine host a caller embeds.

pub mod config;
#[path = "1_dead_route.rs"]
pub mod dead_route;
pub mod deliver;
pub mod headwatch;
pub mod inbox;
pub mod lane;
pub mod mailwait;
#[cfg(unix)]
#[path = "0_resource_guard.rs"]
pub mod resource_guard;
pub mod supervise;

pub use lane::{Effort, LaneIdentity, ModelSpec};
pub use supervise::ParentDeathPolicy;
