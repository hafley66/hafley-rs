//! Deterministic v1 behavior examples carried into the pure resolver.
//!
//! Sources (v1, `0_rust_v1_ship/rust-sim/core/src`):
//! - `physics.rs`: `apply_di`, `HITLAG_PER_DMG`.
//! - `combat.rs`: `strike` (percent-after, knockback, velocity, hitstun, tumble).
//! - `moves/mod.rs`: `knockback_units`, `Hitbox` set-knockback branch.
//! - `di_tests.rs`: DI rotates the angle and preserves speed; neutral is identity.
//!
//! Numeric policy constants are supplied per test: hitlag is 0.8 frames per point
//! of damage plus 4, tumble past 80 knockback units, matching the v1 values.

use game_combat::{resolve_hit, DefenseInput, HitOutcome, Policy, Strike, Target};

fn policy() -> Policy {
    Policy {
        hitlag_per_damage: 0.8,
        hitlag_bonus: 4,
        tumble_threshold: 80.0,
    }
}

fn strike() -> Strike {
    Strike {
        damage: 12.0,
        angle: 45.0,
        base_knockback: 30,
        knockback_growth: 60,
        weight_dependent_set_knockback: 0,
    }
}

fn target(percent: f32, weight: f32, grounded: bool) -> Target {
    Target {
        percent,
        weight,
        grounded,
    }
}

fn neutral() -> DefenseInput {
    DefenseInput { stick: [0.0, 0.0] }
}

fn speed(v: [f32; 2]) -> f32 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

#[test]
fn percent_after_adds_damage() {
    let out = resolve_hit(strike(), target(40.0, 100.0, true), neutral(), policy());
    assert_eq!(out.percent_after, 52.0);
}

#[test]
fn di_neutral_is_identity() {
    let base = resolve_hit(strike(), target(60.0, 100.0, false), neutral(), policy());
    let deadzone = resolve_hit(
        strike(),
        target(60.0, 100.0, false),
        DefenseInput {
            stick: [0.1, 0.1],
        },
        policy(),
    );
    assert_eq!(base.angle, deadzone.angle);
}

#[test]
fn di_rotates_angle_keeps_speed() {
    let victim = target(80.0, 100.0, false);
    let base = resolve_hit(strike(), victim, neutral(), policy());
    let steered = resolve_hit(
        strike(),
        victim,
        DefenseInput { stick: [1.0, 0.0] },
        policy(),
    );
    assert!(
        (speed(base.velocity) - speed(steered.velocity)).abs() < 1e-4,
        "speed must be preserved"
    );
    assert!(
        steered.velocity[0] > 0.0,
        "stick right bends the launch toward +x"
    );
    assert!(steered.angle != base.angle, "DI changed the angle");
}

#[test]
fn di_is_capped_at_18_degrees() {
    let up = Strike {
        angle: 90.0,
        ..strike()
    };
    let base = resolve_hit(up, target(80.0, 100.0, false), neutral(), policy());
    let steered = resolve_hit(
        up,
        target(80.0, 100.0, false),
        DefenseInput { stick: [1.0, 0.0] },
        policy(),
    );
    let delta = (base.angle - steered.angle).abs();
    assert!(
        (delta - 18.0_f32.to_radians()).abs() < 1e-4,
        "DI must hit the 18 degree cap, got {} deg",
        delta.to_degrees()
    );
}

#[test]
fn sakurai_angle_grounded_low_knockback_is_horizontal() {
    let strike = Strike {
        damage: 1.0,
        angle: 361.0,
        base_knockback: 10,
        knockback_growth: 0,
        weight_dependent_set_knockback: 0,
    };
    let out = resolve_hit(strike, target(0.0, 100.0, true), neutral(), policy());
    assert_eq!(out.angle, 0.0);
}

#[test]
fn sakurai_angle_airborne_is_45_degrees() {
    let strike = Strike {
        damage: 1.0,
        angle: 361.0,
        base_knockback: 10,
        knockback_growth: 0,
        weight_dependent_set_knockback: 0,
    };
    let out = resolve_hit(strike, target(0.0, 100.0, false), neutral(), policy());
    assert_eq!(out.angle, 45.0_f32.to_radians());
}

#[test]
fn arbitrary_weight_scales_knockback() {
    let light = resolve_hit(strike(), target(50.0, 100.0, false), neutral(), policy());
    let heavy = resolve_hit(strike(), target(50.0, 200.0, false), neutral(), policy());
    assert!(
        light.knockback > heavy.knockback,
        "heavier targets launch less: {} vs {}",
        light.knockback,
        heavy.knockback
    );
}

#[test]
fn set_knockback_branch_still_applies_weight() {
    let set = Strike {
        damage: 4.0,
        angle: 80.0,
        base_knockback: 8,
        knockback_growth: 100,
        weight_dependent_set_knockback: 6,
    };
    let light = resolve_hit(set, target(0.0, 100.0, true), neutral(), policy());
    let heavy = resolve_hit(set, target(0.0, 200.0, true), neutral(), policy());
    assert!(light.knockback > 0.0 && heavy.knockback > 0.0);
    assert!(light.knockback > heavy.knockback);
}

#[test]
fn hitlag_is_per_damage_plus_bonus() {
    let case = Strike {
        damage: 10.0,
        ..strike()
    };
    let out = resolve_hit(case, target(0.0, 100.0, true), neutral(), policy());
    assert_eq!(out.hitlag, 12);

    let jab = Strike {
        damage: 3.0,
        ..strike()
    };
    let out = resolve_hit(jab, target(0.0, 100.0, true), neutral(), policy());
    assert_eq!(out.hitlag, 6);
}

#[test]
fn hitstun_is_floor_of_knockback_times_0_4() {
    let out = resolve_hit(strike(), target(40.0, 100.0, false), neutral(), policy());
    assert_eq!(out.hitstun, (out.knockback * 0.4).floor() as u32);
}

#[test]
fn tumble_above_threshold() {
    let small = Strike {
        damage: 2.0,
        angle: 45.0,
        base_knockback: 20,
        knockback_growth: 0,
        weight_dependent_set_knockback: 0,
    };
    let big = Strike {
        damage: 20.0,
        angle: 45.0,
        base_knockback: 80,
        knockback_growth: 100,
        weight_dependent_set_knockback: 0,
    };
    let low = resolve_hit(small, target(0.0, 100.0, false), neutral(), policy());
    let high = resolve_hit(big, target(80.0, 100.0, false), neutral(), policy());
    assert!(low.knockback < 80.0 && !low.tumble);
    assert!(high.knockback > 80.0 && high.tumble);
}

#[test]
fn serde_roundtrip() {
    let strike = strike();
    let target = target(40.0, 128.0, true);
    let input = DefenseInput { stick: [0.5, -0.5] };
    let policy = policy();
    let outcome = resolve_hit(strike, target, input, policy);

    assert_eq!(
        strike,
        serde_json::from_str::<Strike>(&serde_json::to_string(&strike).unwrap()).unwrap()
    );
    assert_eq!(
        target,
        serde_json::from_str::<Target>(&serde_json::to_string(&target).unwrap()).unwrap()
    );
    assert_eq!(
        input,
        serde_json::from_str::<DefenseInput>(&serde_json::to_string(&input).unwrap()).unwrap()
    );
    assert_eq!(
        policy,
        serde_json::from_str::<Policy>(&serde_json::to_string(&policy).unwrap()).unwrap()
    );
    assert_eq!(
        outcome,
        serde_json::from_str::<HitOutcome>(&serde_json::to_string(&outcome).unwrap()).unwrap()
    );
}
