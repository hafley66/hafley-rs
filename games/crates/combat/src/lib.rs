//! Pure, deterministic hit resolution for the shared game.
//!
//! One entrypoint, [`resolve_hit`], turns a [`Strike`], a [`Target`], the
//! victim's [`DefenseInput`] and a [`ResolvePolicy`] into a [`HitOutcome`].
//! Knockback, Sakurai angle, DI, hitstun and initial launch velocity come from
//! `ssbm_utils`; the target weight is carried as a plain value so no character
//! enum enters this API. Rapier, Parry, rendering, SQLite, Godot and app state
//! stay outside this crate.

#[path = "0_types.rs"]
mod types;
pub use types::{DefenseInput, HitOutcome, ResolvePolicy, Strike, Target};

#[path = "1_resolve.rs"]
mod resolve;
pub use resolve::*;
