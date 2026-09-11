//! Pure, deterministic hit resolution for the shared game.
//!
//! One entrypoint, [`resolve_hit`], turns a [`Strike`], a [`Target`], the
//! victim's [`DefenseInput`] and a [`ResolvePolicy`] into a [`HitOutcome`].
//! Knockback, Sakurai angle, DI, hitstun and initial launch velocity come from
//! `ssbm_utils`; the target weight is carried as a plain value so no character
//! enum enters this API. Rapier, Parry, rendering, SQLite, Godot and app state
//! stay outside this crate.

use ssbm_utils::{calc, enums::character::Attributes, types::StickPos};

use crate::{DefenseInput, HitOutcome, ResolvePolicy, Strike, Target};

/// Build the `ssbm_utils` attribute view the knockback formula reads. Only
/// `weight` and `name` are consulted; the character enum stays private.
fn attributes_for_weight(weight: f32) -> Attributes<'static> {
    Attributes {
        name: "",
        weight: weight.max(0.0).round() as u32,
        ..Attributes::MARIO
    }
}

/// Resolve the authored angle, including the Sakurai angle.
///
/// `ssbm_utils` 0.4.0 `resolve_sakurai_angle` returns the interpolated window
/// (`32.0 < knockback < 32.1`, grounded) in degrees while every other branch
/// returns radians. Normalize that one window before DI consumes the angle.
fn resolve_angle(strike: &Strike, knockback: f32, grounded: bool) -> f32 {
    let authored = strike.angle.to_radians();
    let resolved = calc::resolve_sakurai_angle(authored, knockback, grounded);
    let interpolated = authored == 361.0_f32.to_radians()
        && grounded
        && knockback > 32.0
        && knockback < 32.1;
    if interpolated {
        resolved.to_radians()
    } else {
        resolved
    }
}

/// Resolve one connect. Pure: no mutation, no allocation, no I/O.
pub fn resolve_hit(
    strike: Strike,
    target: Target,
    input: DefenseInput,
    policy: ResolvePolicy,
) -> HitOutcome {
    let percent_after = target.percent + strike.damage;
    let attributes = attributes_for_weight(target.weight);
    let knockback = calc::knockback(
        strike.damage,
        strike.damage,
        strike.knockback_growth,
        strike.base_knockback,
        strike.weight_dependent_set_knockback,
        false,
        &attributes,
        target.percent,
        false,
        false,
        false,
        false,
        false,
        false,
    );
    let angle = resolve_angle(&strike, knockback, target.grounded);
    let angle = calc::apply_di(angle, StickPos::new(input.stick[0], input.stick[1]));
    let velocity = [
        calc::initial_x_velocity(knockback, angle),
        calc::initial_y_velocity(knockback, angle, target.grounded),
    ];
    let hitstun = calc::hitstun(knockback);
    let hitlag = (strike.damage * policy.hitlag_per_damage) as u32 + policy.hitlag_bonus;
    let tumble = knockback > policy.tumble_knockback;

    HitOutcome {
        percent_after,
        angle,
        velocity,
        knockback,
        hitlag,
        hitstun,
        tumble,
    }
}
