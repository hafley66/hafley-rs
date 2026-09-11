//! The one pure hit-resolution entrypoint. Fixed sequence: damage -> knockback units ->
//! Sakurai angle -> DI -> launch vector -> hitstun/hitlag/tumble.
//!
//! `ssbm_utils::calc::knockback` is not used: it requires a character-derived
//! `Attributes` with an integer weight, so it cannot preserve v1's arbitrary `f32`
//! target weight. The community/PM formula is ported verbatim over `f32`. Sakurai angle,
//! DI, hitstun and the initial-velocity helpers come from `ssbm_utils::calc`.

use ssbm_utils::{calc, types::StickPos};

use crate::types::{DefenseInput, Launch, Policy, Strike, Target};

/// Resolve one strike against one target. Pure; no state is read or written.
pub fn resolve_hit(strike: Strike, target: Target, input: DefenseInput, policy: Policy) -> Launch {
    let percent_after = target.percent + strike.damage;
    let knockback = knockback_units(percent_after, strike, target.weight);
    let angle =
        calc::resolve_sakurai_angle(strike.angle.to_radians(), knockback, target.grounded);
    let angle = calc::apply_di(angle, StickPos::new(input.stick[0], input.stick[1]));
    // ssbm_utils uses math coordinates (y up); the sim uses y down, so the y term flips.
    let velocity = [
        calc::initial_x_velocity(knockback, angle),
        -calc::initial_y_velocity(knockback, angle, target.grounded),
    ];
    Launch {
        percent_after,
        damage: strike.damage,
        knockback,
        angle,
        velocity,
        hitlag: (strike.damage * policy.hitlag_per_damage) as u32 + policy.hitlag_bonus,
        hitstun: calc::hitstun(knockback),
        tumble: knockback > policy.tumble_knockback,
    }
}

/// Community / Project-M knockback in KB units over an arbitrary `f32` weight. With a
/// non-zero `weight_dependent_set_knockback` the damage/percent ramp is replaced by the
/// set-knockback term; weight, growth and base still apply.
fn knockback_units(percent_after: f32, strike: Strike, weight: f32) -> f32 {
    let weight_term = 200.0 / (weight + 100.0);
    let growth = strike.knockback_growth as f32 / 100.0;
    let base = strike.base_knockback as f32;
    let ramp = if strike.weight_dependent_set_knockback > 0 {
        (strike.weight_dependent_set_knockback * 10 / 20) as f32 + 1.0
    } else {
        percent_after / 10.0 + percent_after * strike.damage / 20.0
    };
    (ramp * weight_term * 1.4 + 18.0) * growth + base
}
