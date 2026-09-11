//! Deterministic behavior examples ported from the v1 resolver
//! (`rust-sim/core/src/combat.rs`, `moves/mod.rs::knockback_units`,
//! `physics.rs::apply_di`, `di_tests.rs`). Expected numbers are computed against those
//! v1 sources; the pipeline swaps in `ssbm_utils` where the operation is identical.

use game_combat::{resolve_hit, DefenseInput, Launch, Policy, Strike, Target};

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-4
}

fn neutral() -> DefenseInput {
    DefenseInput { stick: [0.0, 0.0] }
}

/// v1 `Tune` defaults used by the ported scenarios: HITLAG_PER_DMG = 0.8, melee-grade
/// hitlag_bonus = 4, and tumble_speed / (kb_speed * knockback_mult) = 620 / 8.4.
fn policy() -> Policy {
    Policy {
        hitlag_per_damage: 0.8,
        hitlag_bonus: 4,
        tumble_knockback: 73.8,
    }
}

/// v1 `AttackData::JAB` third box (the launcher): dmg 6, angle 78, bkb 30, kbg 40.
fn jab_launcher() -> Strike {
    Strike {
        damage: 6.0,
        angle: 78.0,
        base_knockback: 30,
        knockback_growth: 40,
        weight_dependent_set_knockback: 0,
    }
}

fn mario(percent: f32, grounded: bool) -> Target {
    Target {
        percent,
        weight: 100.0,
        grounded,
    }
}

#[test]
fn jab_launcher_matches_v1() {
    let l = resolve_hit(jab_launcher(), mario(0.0, false), neutral(), policy());
    assert!(close(l.percent_after, 6.0), "percent_after {}", l.percent_after);
    assert!(close(l.damage, 6.0));
    assert!(close(l.knockback, 38.544), "knockback {}", l.knockback);
    assert!(close(l.angle, 78.0_f32.to_radians()));
    assert!(
        close(l.velocity[0], 0.240_412_4) && close(l.velocity[1], -1.131_051_6),
        "velocity {:?}",
        l.velocity
    );
    assert_eq!(l.hitlag, 8, "floor(6 * 0.8) + 4");
    assert_eq!(l.hitstun, 15, "floor(0.4 * knockback)");
    assert!(!l.tumble, "38.544 < 73.8");
}

#[test]
fn percent_after_accumulates_target_damage() {
    let l = resolve_hit(jab_launcher(), mario(40.0, false), neutral(), policy());
    assert!(close(l.percent_after, 46.0));
}

#[test]
fn knockback_uses_arbitrary_f32_weight() {
    let light = resolve_hit(
        jab_launcher(),
        Target {
            percent: 0.0,
            weight: 60.0,
            grounded: false,
        },
        neutral(),
        policy(),
    );
    let heavy = resolve_hit(
        jab_launcher(),
        Target {
            percent: 0.0,
            weight: 120.0,
            grounded: false,
        },
        neutral(),
        policy(),
    );
    assert!(close(light.knockback, 38.88), "{}", light.knockback);
    assert!(close(heavy.knockback, 38.421_818), "{}", heavy.knockback);
    assert!(light.knockback > heavy.knockback, "heavier target flies less");
}

#[test]
fn weight_dependent_set_knockback_replaces_percent_ramp() {
    let strike = Strike {
        damage: 9.0,
        angle: 80.0,
        base_knockback: 8,
        knockback_growth: 40,
        weight_dependent_set_knockback: 12,
    };
    let at_zero = resolve_hit(strike, mario(0.0, false), neutral(), policy());
    let at_high_percent = resolve_hit(strike, mario(99.0, false), neutral(), policy());
    assert!(close(at_zero.knockback, 19.12), "{}", at_zero.knockback);
    assert!(close(at_high_percent.knockback, 19.12));
}

#[test]
fn sakurai_angle_airborne_is_45_degrees() {
    let strike = Strike {
        damage: 8.0,
        angle: 361.0,
        base_knockback: 20,
        knockback_growth: 50,
        weight_dependent_set_knockback: 0,
    };
    let l = resolve_hit(strike, mario(9.0, false), neutral(), policy());
    assert!(close(l.angle, 45.0_f32.to_radians()), "{}", l.angle);
}

#[test]
fn sakurai_angle_grounded_high_knockback_is_44_degrees() {
    let strike = Strike {
        damage: 8.0,
        angle: 361.0,
        base_knockback: 20,
        knockback_growth: 50,
        weight_dependent_set_knockback: 0,
    };
    let l = resolve_hit(strike, mario(9.0, true), neutral(), policy());
    assert!(close(l.knockback, 34.95), "{}", l.knockback);
    assert!(close(l.angle, 44.0_f32.to_radians()), "{}", l.angle);
}

#[test]
fn sakurai_angle_grounded_low_knockback_is_horizontal() {
    let strike = Strike {
        damage: 4.0,
        angle: 361.0,
        base_knockback: 20,
        knockback_growth: 50,
        weight_dependent_set_knockback: 0,
    };
    let l = resolve_hit(strike, mario(9.0, true), neutral(), policy());
    assert!(close(l.knockback, 31.73), "{}", l.knockback);
    assert!(close(l.angle, 0.0), "{}", l.angle);
}

#[test]
fn di_bends_angle_and_keeps_velocity_magnitude() {
    let launch = Strike {
        damage: 6.0,
        angle: 90.0,
        base_knockback: 30,
        knockback_growth: 40,
        weight_dependent_set_knockback: 0,
    };
    let neutral_hit = resolve_hit(launch, mario(0.0, false), neutral(), policy());
    assert!(
        neutral_hit.velocity[0].abs() < 1e-4 && close(neutral_hit.velocity[1], -1.156_32),
        "straight up {:?}",
        neutral_hit.velocity
    );

    let steered = resolve_hit(
        launch,
        mario(0.0, false),
        DefenseInput { stick: [1.0, 0.0] },
        policy(),
    );
    assert!(close(steered.angle, 72.0_f32.to_radians()), "{}", steered.angle);
    assert!(
        close(steered.velocity[0], 0.357_322_5) && close(steered.velocity[1], -1.099_725_7),
        "{:?}",
        steered.velocity
    );
}

#[test]
fn hitlag_comes_from_policy() {
    let l = resolve_hit(jab_launcher(), mario(0.0, false), neutral(), policy());
    assert_eq!(l.hitlag, 8);

    let heavy = Policy {
        hitlag_per_damage: 0.8,
        hitlag_bonus: 0,
        tumble_knockback: 73.8,
    };
    let big = Strike {
        damage: 12.0,
        ..jab_launcher()
    };
    let l = resolve_hit(big, mario(0.0, false), neutral(), heavy);
    assert_eq!(l.hitlag, 9, "floor(12 * 0.8)");
}

#[test]
fn tumble_reads_policy_threshold() {
    let low = Policy {
        tumble_knockback: 20.0,
        ..policy()
    };
    let l = resolve_hit(jab_launcher(), mario(0.0, false), neutral(), low);
    assert!(l.tumble);

    let l = resolve_hit(jab_launcher(), mario(0.0, false), neutral(), policy());
    assert!(!l.tumble);
}

#[test]
fn data_round_trips_through_serde() {
    let launch: Launch = resolve_hit(jab_launcher(), mario(0.0, false), neutral(), policy());
    let json = serde_json::to_string(&launch).unwrap();
    let back: Launch = serde_json::from_str(&json).unwrap();
    assert_eq!(launch, back);
}
