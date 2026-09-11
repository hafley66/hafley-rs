//! `game-combat`: pure, deterministic hit resolution.
//!
//! Owns one connect's math and nothing else. Fighter state, input history, collision
//! detection, rigid-body integration, rendering and content ingestion stay out.

#[path = "0_types.rs"]
mod types;
#[path = "1_resolve.rs"]
mod resolve;

pub use resolve::resolve_hit;
pub use types::{DefenseInput, Launch, Policy, Strike, Target};
