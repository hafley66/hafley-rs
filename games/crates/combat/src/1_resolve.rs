//! Pure resolution of one strike against one target.

use crate::{DefenseInput, HitOutcome, Policy, Strike, Target};
use ssbm_utils::calc;
use ssbm_utils::enums::character::Attributes;
use ssbm_utils::types::StickPos;

/// A Melee attribute record carrying only the target's weight. The knockback
/// helper reads `weight` (and, for Nana, `name`); every other field is inert.
fn attributes_for(weight: f32) -> Attributes<'static> {
    let mut attributes = Attributes::MARIO;
    attributes.weight = weight.max(0.0).round() as u32;
    attributes
}

/// Resolve one hit. Pure: no state is read or written outside the arguments.
///
/// Knockback, Sakurai angle, DI, initial velocity, and hitstun come from
/// `ssbm_utils` 0.4.0. Hitlag and tumble read the caller's [`Policy`]. The
/// target's arbitrary weight is carried through by weight alone, with no
/// character enum in the public API.
pub fn resolve_hit(
    strike: Strike,
    target: Target,
    input: DefenseInput,
    policy: Policy,
) -> HitOutcome {
    let percent_after = target.percent + strike.damage;

    let attributes = attributes_for(target.weight);
    let knockback = calc::knockback(
        strike.damage,
        strike.damage,
        strike.knockback_growth,
        strike.base_knockback,
        strike.weight_dependent_set_knockback,
        false,
        &attributes,
        percent_after,
        false,
        false,
        false,
        false,
        false,
        false,
    );

    let base_angle = calc::resolve_sakurai_angle(
        strike.angle.to_radians(),
        knockback,
        target.grounded,
    );
    let angle = calc::apply_di(base_angle, StickPos::new(input.stick[0], input.stick[1]));

    HitOutcome {
        percent_after,
        angle,
        velocity: [
            calc::initial_x_velocity(knockback, angle),
            calc::initial_y_velocity(knockback, angle, target.grounded),
        ],
        knockback,
        hitlag: (strike.damage * policy.hitlag_per_damage) as u32 + policy.hitlag_bonus,
        hitstun: calc::hitstun(knockback),
        tumble: knockback > policy.tumble_threshold,
    }
}
