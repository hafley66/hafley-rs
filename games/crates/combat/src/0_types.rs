//! Plain serde data for one pure hit resolution.
//!
//! Ported from the v1 combat resolver: `combat::Launch` and the knockback
//! record read by `moves::knockback_units` (`rust-sim/core/src/combat.rs`,
//! `rust-sim/core/src/moves/mod.rs`). This crate owns no fighter, input,
//! collision, physics, rendering, SQLite, or app state.

use serde::{Deserialize, Serialize};

/// One attacker hitbox's launch record, the inputs the v1 `knockback_units`
/// function reads off a `moves::Hitbox`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Strike {
    /// Damage added to the target percent on connect, in percent points.
    pub damage: f32,
    /// Authored launch angle in degrees. `361.0` selects the Sakurai angle.
    pub angle: f32,
    /// v1 `Hitbox::bkb`, base knockback in knockback units.
    pub base_knockback: u32,
    /// v1 `Hitbox::kbg`, knockback growth in knockback units.
    pub knockback_growth: u32,
    /// v1 `Hitbox::set_kb`, fixed set knockback in knockback units; `0`
    /// selects the growth formula.
    pub weight_dependent_set_knockback: u32,
}

/// The receiver's combat state the v1 `PunchableFace::percent` / `heft` /
/// `Launch` readers exposed.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Target {
    /// Accumulated damage percent before this hit.
    pub percent: f32,
    /// Target weight, the v1 `heft` term of the knockback formula.
    pub weight: f32,
    /// Whether the target is on the ground, feeding the Sakurai angle and the
    /// grounded initial-velocity rule.
    pub grounded: bool,
}

/// The victim's defensive input for this hit: the stick read by trajectory DI.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DefenseInput {
    /// Stick position, `[x, y]`, y up.
    pub stick: [f32; 2],
}

/// Named ruleset inputs for the policies whose v1 source is not yet qualified.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvePolicy {
    /// Hitlag frames per point of damage, the v1 `HITLAG_PER_DMG` knob.
    pub hitlag_per_damage: f32,
    /// Flat hitlag frames added after the per-damage term.
    pub hitlag_bonus: u32,
    /// Launches above this knockback tumble, the v1 `tumble_speed` threshold.
    pub tumble_knockback: f32,
}

impl Default for ResolvePolicy {
    fn default() -> Self {
        Self {
            hitlag_per_damage: 0.8,
            hitlag_bonus: 0,
            tumble_knockback: 80.0,
        }
    }
}

/// What one connect does to one target, fully computed before anything mutates.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HitOutcome {
    /// Target percent after the hit's damage is added.
    pub percent_after: f32,
    /// Resolved launch angle in radians, after Sakurai resolution and DI.
    pub angle: f32,
    /// Initial launch velocity `[x, y]`, y up, from the qualified helpers.
    pub velocity: [f32; 2],
    /// Knockback in knockback units.
    pub knockback: f32,
    /// Impact freeze frames.
    pub hitlag: u32,
    /// Frames the target spends in hitstun.
    pub hitstun: u32,
    /// Whether the launch tumbles the target.
    pub tumble: bool,
}
