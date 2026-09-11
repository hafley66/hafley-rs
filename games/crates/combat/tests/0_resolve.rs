//! Deterministic resolver examples ported from the v1 combat cut.
//!
//! Each case names the v1 source symbol it exercises and pins the exact
//! output, so a change in behavior fails here rather than in a downstream
//! fighter reducer. Knockback, angle and hitstun vectors match the
//! `ssbm_utils` tests the qualified helpers ship with.

use game_combat::{
    DefenseInput, HitOutcome, ResolvePolicy, Strike, Target, resolve_hit,
};

fn strike(damage: f32, angle: f32, base_knockback: u32, knockback_growth: u32) -> Strike {
    Strike {
        damage,
        angle,
        base_knockback,
        knockback_growth,
        weight_dependent_set_knockback: 0,
    }
}

fn neutral() -> DefenseInput {
    DefenseInput { stick: [0.0, 0.0] }
}

fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

// v1 `PunchableFace::absorb`: `self.damage += l.dmg`.
#[test]
fn connect_adds_damage_to_percent() {
    let outcome = resolve_hit(
        strike(8.0, 45.0, 30, 40),
        Target {
            percent: 30.0,
            weight: 100.0,
            grounded: true,
        },
        neutral(),
        ResolvePolicy::default(),
    );
    assert!(close(outcome.percent_after, 38.0, 1e-6));
}

// v1 `moves::knockback_units` growth branch, qualified through
// `ssbm_utils::calc::knockback`. Marth tipper forward smash against Fox at
// 80%, the vector carried by the `ssbm_utils` knockback test.
#[test]
fn growth_knockback_matches_qualified_vector() {
    let outcome = resolve_hit(
        strike(20.0, 361.0, 80, 70),
        Target {
            percent: 80.0,
            weight: 75.0,
            grounded: false,
        },
        neutral(),
        ResolvePolicy::default(),
    );
    assert!(close(outcome.knockback, 215.8, 1e-3), "kb={}", outcome.knockback);
    assert_eq!(outcome.hitstun, 86);
    assert!(
        close(outcome.angle.to_degrees(), 45.0, 1e-3),
        "airborne Sakurai angle, got {}",
        outcome.angle.to_degrees()
    );
}

// Falco shine knockback, the second `ssbm_utils` knockback test vector.
#[test]
fn shine_knockback_matches_qualified_vector() {
    let outcome = resolve_hit(
        strike(8.0, 84.0, 110, 50),
        Target {
            percent: 80.0,
            weight: 75.0,
            grounded: false,
        },
        neutral(),
        ResolvePolicy::default(),
    );
    assert!(close(outcome.knockback, 154.2, 1e-3), "kb={}", outcome.knockback);
    assert_eq!(outcome.hitstun, 61);
}

// v1 `physics::apply_di` survival DI: counter-hold rotates the trajectory by
// the full 18 degree cap and never changes the knockback magnitude.
#[test]
fn di_bends_angle_keeps_knockback() {
    let launch = strike(10.0, 90.0, 40, 80);
    let target = Target {
        percent: 50.0,
        weight: 90.0,
        grounded: false,
    };
    let straight = resolve_hit(launch, target, neutral(), ResolvePolicy::default());
    let steered = resolve_hit(
        launch,
        target,
        DefenseInput { stick: [1.0, 0.0] },
        ResolvePolicy::default(),
    );

    assert!(
        close(straight.angle.to_degrees(), 90.0, 1e-3),
        "neutral stick launches straight up"
    );
    assert!(
        close(steered.angle.to_degrees(), 72.0, 1e-3),
        "stick right bends the vertical launch 18 degrees, got {}",
        steered.angle.to_degrees()
    );
    assert!(close(straight.knockback, steered.knockback, 1e-4));
    assert!(steered.velocity[0] > 0.0, "bent toward +x");
}

// v1 `moves::Hitbox::angle == 361` Sakurai resolution: grounded, above the
// high threshold, the hit sends 44 degrees. `ssbm_utils` sourspot-jab vector.
#[test]
fn sakurai_angle_grounded_matches_qualified_vector() {
    let outcome = resolve_hit(
        strike(4.0, 361.0, 20, 50),
        Target {
            percent: 9.0,
            weight: 80.0,
            grounded: true,
        },
        neutral(),
        ResolvePolicy::default(),
    );
    assert!(close(outcome.knockback, 32.033_333, 1e-3));
    assert!(
        close(outcome.angle.to_degrees(), 14.666_443, 1e-2),
        "got {}",
        outcome.angle.to_degrees()
    );
    assert_eq!(outcome.hitstun, 12);
}

// v1 `moves::Hitbox::set_kb > 0` selects the set-knockback branch.
#[test]
fn set_knockback_input_selects_set_branch() {
    let outcome = resolve_hit(
        Strike {
            damage: 3.0,
            angle: 80.0,
            base_knockback: 8,
            knockback_growth: 100,
            weight_dependent_set_knockback: 7,
        },
        Target {
            percent: 20.0,
            weight: 100.0,
            grounded: true,
        },
        neutral(),
        ResolvePolicy::default(),
    );
    assert!(close(outcome.knockback, 31.6, 1e-3), "kb={}", outcome.knockback);
}

// v1 `physics::HITLAG_PER_DMG` plus the caller's flat impact bonus.
#[test]
fn hitlag_uses_policy_knobs() {
    let outcome = resolve_hit(
        strike(10.0, 45.0, 30, 40),
        Target {
            percent: 0.0,
            weight: 100.0,
            grounded: true,
        },
        neutral(),
        ResolvePolicy {
            hitlag_per_damage: 0.8,
            hitlag_bonus: 4,
            tumble_knockback: 80.0,
        },
    );
    assert_eq!(outcome.hitlag, 12);
}

// v1 `Launch::tumble` threshold, now a named policy input.
#[test]
fn tumble_tracks_policy_threshold() {
    let hard = resolve_hit(
        strike(20.0, 361.0, 80, 70),
        Target {
            percent: 80.0,
            weight: 75.0,
            grounded: false,
        },
        neutral(),
        ResolvePolicy::default(),
    );
    let soft = resolve_hit(
        strike(4.0, 361.0, 20, 50),
        Target {
            percent: 9.0,
            weight: 80.0,
            grounded: true,
        },
        neutral(),
        ResolvePolicy::default(),
    );
    assert!(hard.tumble);
    assert!(!soft.tumble);
}

// v1 `Tune::weight_of` fed the formula rather than a hardcoded character.
// Heavier targets take less knockback from the same strike.
#[test]
fn target_weight_feeds_knockback() {
    let launch = strike(10.0, 45.0, 40, 80);
    let light = resolve_hit(
        launch,
        Target {
            percent: 50.0,
            weight: 90.0,
            grounded: false,
        },
        neutral(),
        ResolvePolicy::default(),
    );
    let heavy = resolve_hit(
        launch,
        Target {
            percent: 50.0,
            weight: 100.0,
            grounded: false,
        },
        neutral(),
        ResolvePolicy::default(),
    );
    assert!(light.knockback > heavy.knockback);
}

// The public data crosses the serde boundary unchanged.
#[test]
fn outcome_round_trips_through_serde() {
    let outcome: HitOutcome = resolve_hit(
        strike(12.0, 361.0, 20, 90),
        Target {
            percent: 40.0,
            weight: 95.0,
            grounded: true,
        },
        DefenseInput { stick: [0.5, -0.5] },
        ResolvePolicy::default(),
    );
    let encoded = serde_json::to_string(&outcome).unwrap();
    let decoded: HitOutcome = serde_json::from_str(&encoded).unwrap();
    assert_eq!(outcome, decoded);
}
