//! Plain serde data for one hit: the strike the attacker authors, the target state it
//! lands on, the victim's defense input, and the ruleset policy that names the hitlag and
//! tumble rules. No fighter state, physics body, renderer or app type lives here.

use serde::{Deserialize, Serialize};

/// One attack's launch payoff. `angle` is degrees (0 forward, 90 up, negative down); 361
/// requests the Sakurai angle. Knockback fields are in community/PM KB units.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Strike {
    pub damage: f32,
    pub angle: f32,
    pub base_knockback: u32,
    pub knockback_growth: u32,
    /// Melee-style weight-dependent set knockback. When non-zero it replaces the
    /// damage/percent ramp; weight, `knockback_growth` and `base_knockback` still apply.
    pub weight_dependent_set_knockback: u32,
}

/// The receiving body's relevant state: accumulated damage, weight and grounded flag.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Target {
    pub percent: f32,
    pub weight: f32,
    pub grounded: bool,
}

/// The victim's input at the moment of the hit. `stick` is the raw analog stick, y down.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DefenseInput {
    pub stick: [f32; 2],
}

/// Named ruleset inputs whose exact v1 source is not yet qualified. Kept explicit so no
/// v1 numeric approximation silently becomes authority.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Policy {
    /// Impact-freeze frames per point of damage.
    pub hitlag_per_damage: f32,
    /// Flat impact-freeze frames added on top of the per-damage term.
    pub hitlag_bonus: u32,
    /// Knockback (KB units) above which the launch tumbles.
    pub tumble_knockback: f32,
}

/// The fully computed result of one connect. `velocity` is in units/frame with y down.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Launch {
    pub percent_after: f32,
    pub damage: f32,
    pub knockback: f32,
    pub angle: f32,
    pub velocity: [f32; 2],
    pub hitlag: u32,
    pub hitstun: u32,
    pub tumble: bool,
}
