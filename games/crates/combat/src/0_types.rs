//! Plain serde values for one hit. No engine, character, or asset types cross
//! these boundaries; a caller supplies the strike data, the target's percent and
//! weight, the victim's stick, and its ruleset policy.

use serde::{Deserialize, Serialize};

/// One attack's connect data authored on a hitbox.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Strike {
    /// Percent added on connect.
    pub damage: f32,
    /// Authored launch angle in degrees. `361.0` selects the Sakurai angle.
    pub angle: f32,
    /// Base knockback (BKB), knockback units.
    pub base_knockback: u32,
    /// Knockback growth (KBG), percent of the growth ramp.
    pub knockback_growth: u32,
    /// Fixed knockback branch (WDSK), knockback units. `0` uses the growth formula.
    pub weight_dependent_set_knockback: u32,
}

/// The receiving body's combat state at contact.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Target {
    /// Accumulated percent before this hit.
    pub percent: f32,
    /// Arbitrary target weight. Rounded to the knockback helper's integer weight.
    pub weight: f32,
    /// Grounded at contact. Selects the Sakurai-angle branch and the vertical
    /// ground rule in the initial-velocity helper.
    pub grounded: bool,
}

/// The victim's trajectory input at contact.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DefenseInput {
    /// Stick position, each axis in `[-1, 1]`.
    pub stick: [f32; 2],
}

/// Named ruleset inputs whose source is not yet qualified. These are explicit
/// caller values, not baked-in constants.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Policy {
    /// Hitlag frames per point of damage.
    pub hitlag_per_damage: f32,
    /// Flat hitlag frames added after the damage term.
    pub hitlag_bonus: u32,
    /// Knockback units at or above which the launch tumbles.
    pub tumble_threshold: f32,
}

/// Fully resolved result of one connect.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HitOutcome {
    /// Percent after the hit's damage is applied.
    pub percent_after: f32,
    /// Resolved launch angle in radians, after Sakurai-angle and DI.
    pub angle: f32,
    /// Initial launch velocity, `[x, y]`, +x right and +y up.
    pub velocity: [f32; 2],
    /// Knockback value in knockback units.
    pub knockback: f32,
    /// Hitlag frames.
    pub hitlag: u32,
    /// Hitstun frames.
    pub hitstun: u32,
    /// Launch tumbles (drives knockdown on landing).
    pub tumble: bool,
}
