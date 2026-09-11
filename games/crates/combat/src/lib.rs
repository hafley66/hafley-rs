//! Pure, engine-free hit resolution.
//!
//! `game-combat` owns one connect's math: damage, knockback, Sakurai angle, DI,
//! launch velocity, hitlag, hitstun and tumble. It owns no fighter state, input,
//! collision, rigid-body integration, rendering or content ingestion. Rapier,
//! Parry, SQLite, Godot and app state stay outside this crate.
//!
//! Knockback and launch math come from `ssbm_utils` 0.4.0. Hitlag and tumble are
//! caller policy; see [`Policy`].

#[path = "0_types.rs"]
mod types;
pub use types::{DefenseInput, HitOutcome, Policy, Strike, Target};

#[path = "1_resolve.rs"]
mod resolve;
pub use resolve::resolve_hit;
