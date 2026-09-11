//! Shared fighter combat response: hit application, hitlag freezing, hitstun
//! countdown, launch motion, tumble and quantized two-axis DI, each with an
//! exact save/serde/replay check through the Redux dispatcher.

use game_combat::{DefenseInput, ResolvePolicy, Strike, Target};
use game_fighter::{
    FighterEvent, FighterSlice, Hit, Input, Phase, Rules, State, advance, apply_hit,
};
use redux::Slice;

fn rules() -> Rules {
    Rules {
        walk_init_vel: 0.4,
        walk_accel: 0.1,
        walk_max_vel: 0.8,
        walk_stick_threshold: 0.3,
        dash_stick_threshold: 0.8,
        dash_initial_velocity: 2.0,
        dash_accel_base: 0.05,
        dash_accel_mul: 0.03,
        dash_max_velocity: 1.6,
        dash_ticks: 4,
        ground_friction: 0.1,
        dash_friction_mul: 0.5,
        ground_max_horizontal_velocity: 4.0,
        turn_ticks: 3,
        jump_startup_time: 4,
        crouch_enter_ticks: 3,
        crouch_exit_ticks: 2,
        jump_h_initial_velocity: 0.5,
        jump_h_max_velocity: 1.2,
        jump_v_initial_velocity: 3.0,
        hop_v_initial_velocity: 2.0,
        ground_to_air_jump_momentum_multiplier: 0.5,
        max_jumps: 2,
        air_jump_v_multiplier: 1.0,
        air_jump_h_multiplier: 0.9,
        gravity: 0.2,
        terminal_velocity: 2.0,
        fast_fall_velocity: 2.8,
        air_drift_stick_mul: 0.06,
        air_drift_base: 0.02,
        air_drift_max: 1.0,
        aerial_friction: 0.02,
        landing_lag: 4,
    }
}

fn idle() -> Input {
    Input {
        axis: 0.0,
        buttons: 0,
    }
}

fn close_degrees(angle: f32, degrees: f32) -> bool {
    (angle.to_degrees() - degrees).abs() <= 1e-3
}

fn strike(damage: f32, angle: f32, base_knockback: u32, knockback_growth: u32) -> Strike {
    Strike {
        damage,
        angle,
        base_knockback,
        knockback_growth,
        weight_dependent_set_knockback: 0,
    }
}

/// A hit with the default policy and a quantized stick.
fn hit(strike: Strike, weight: f32, axes: [i8; 4], buttons: u32) -> Hit {
    Hit {
        strike,
        weight,
        policy: ResolvePolicy::default(),
        input: game_input::PlayerInput { axes, buttons },
    }
}

/// A strong Marth-tipper-grade launch: knockback 215.8, tumbling, long hitstun.
fn strong_hit() -> Hit {
    hit(strike(20.0, 361.0, 80, 70), 75.0, [0, 0, 0, 0], 0)
}

fn dispatch(state: &mut State, event: FighterEvent, rules: &Rules) {
    <FighterSlice as Slice>::reduce(state, event, rules, &mut |never| match never {});
}

fn run(rules: &Rules, tape: &[FighterEvent]) -> Vec<State> {
    let mut state = State::new(rules);
    let mut states = Vec::new();
    for event in tape {
        dispatch(&mut state, *event, rules);
        states.push(state.clone());
    }
    states
}

#[test]
fn hit_applies_percent_and_launch_to_state() {
    let rules = rules();
    let mut state = State::new(&rules);
    let hit = strong_hit();
    let expected = game_combat::resolve_hit(
        hit.strike,
        Target {
            percent: state.combat.percent,
            weight: hit.weight,
            grounded: state.grounded(),
        },
        hit.defense_input(),
        hit.policy,
    );
    let outcome = apply_hit(&mut state, &hit);
    assert_eq!(outcome, expected);
    assert_eq!(state.combat.percent, expected.percent_after);
    assert_eq!(state.combat.hitlag, expected.hitlag);
    assert_eq!(state.combat.hitstun, expected.hitstun);
    assert_eq!(state.combat.knockback, expected.knockback);
    assert_eq!(state.combat.tumble, expected.tumble);
    assert_eq!(state.velocity, expected.velocity);
}

#[test]
fn slice_dispatch_matches_apply_hit() {
    let rules = rules();
    let mut via_seam = State::new(&rules);
    let mut via_slice = State::new(&rules);
    let outcome = apply_hit(&mut via_seam, &strong_hit());
    dispatch(&mut via_slice, FighterEvent::Hit(strong_hit()), &rules);
    assert_eq!(via_slice, via_seam);
    assert_eq!(via_slice.combat.percent, outcome.percent_after);
}

#[test]
fn hitlag_freezes_integration_for_exact_frame_count() {
    let rules = rules();
    let mut state = State::new(&rules);
    apply_hit(&mut state, &strong_hit());
    let frames = state.combat.hitlag;
    assert!(frames >= 1, "the strong hit stores hitlag, got {frames}");
    let frozen_position = state.position;
    let frozen_velocity = state.velocity;
    for tick in 0..frames {
        advance(&mut state, idle(), &rules);
        assert_eq!(state.position, frozen_position, "position moved on tick {tick}");
        assert_eq!(state.velocity, frozen_velocity, "velocity moved on tick {tick}");
    }
    assert_eq!(state.combat.hitlag, 0);
    advance(&mut state, idle(), &rules);
    assert_ne!(state.position, frozen_position, "integration resumes after hitlag");
}

#[test]
fn hitstun_counts_down_and_launch_motion_advances() {
    let rules = rules();
    let mut state = State::new(&rules);
    apply_hit(&mut state, &strong_hit());
    for _ in 0..state.combat.hitlag {
        advance(&mut state, idle(), &rules);
    }
    let launched = state.combat.hitstun;
    assert!(launched > 0, "the strong hit stores hitstun, got {launched}");

    // One launch tick moves x by exactly the launch velocity x; no input control.
    let velocity_x = state.velocity[0];
    let before = state.position;
    advance(&mut state, idle(), &rules);
    assert_eq!(state.position[0], before[0] + velocity_x);
    assert_eq!(state.combat.hitstun, launched - 1);

    let mut previous = state.combat.hitstun;
    let mut ticks = 1;
    while state.combat.hitstun > 0 && ticks < 1000 {
        advance(&mut state, idle(), &rules);
        assert!(state.combat.hitstun <= previous, "hitstun rose");
        previous = state.combat.hitstun;
        ticks += 1;
    }
    assert_eq!(state.combat.hitstun, 0);
}

#[test]
fn tumble_tracks_the_launch_threshold() {
    let rules = rules();
    let mut weak = State::new(&rules);
    let weak_outcome = apply_hit(
        &mut weak,
        &hit(strike(5.0, 45.0, 10, 20), 100.0, [0, 0, 0, 0], 0),
    );
    assert!(!weak_outcome.tumble);
    assert!(!weak.combat.tumble);

    let mut strong = State::new(&rules);
    strong.combat.percent = 80.0;
    let strong_outcome = apply_hit(&mut strong, &strong_hit());
    assert!(strong_outcome.tumble);
    assert!(strong.combat.tumble);
}

#[test]
fn quantized_neutral_and_perpendicular_di_resolve_differently() {
    let rules = rules();
    let mut base = State::new(&rules);
    base.phase = Phase::Fall;
    base.combat.percent = 50.0;
    let strike = strike(10.0, 90.0, 40, 80);
    let weight = 90.0;

    let neutral = hit(strike, weight, [0, 0, 0, 0], 0);
    let mut neutral_state = base.clone();
    let neutral_outcome = apply_hit(&mut neutral_state, &neutral);
    let expected_neutral = game_combat::resolve_hit(
        strike,
        Target {
            percent: base.combat.percent,
            weight,
            grounded: base.grounded(),
        },
        DefenseInput { stick: [0.0, 0.0] },
        ResolvePolicy::default(),
    );
    assert_eq!(neutral_outcome.velocity, expected_neutral.velocity);
    assert!(
        close_degrees(neutral_outcome.angle, 90.0),
        "neutral stick launches straight up, got {}",
        neutral_outcome.angle.to_degrees()
    );

    // Perpendicular stick: horizontal against a vertical launch bends 18 degrees.
    let perpendicular = hit(strike, weight, [game_input::quantize_axis(1.0), 0, 0, 0], 0);
    assert_eq!(perpendicular.defense_input().stick, [1.0, 0.0]);
    let mut perpendicular_state = base.clone();
    let perpendicular_outcome = apply_hit(&mut perpendicular_state, &perpendicular);
    assert_ne!(perpendicular_outcome.velocity, neutral_outcome.velocity);
    assert!(perpendicular_outcome.velocity[0] > 0.0, "bent toward +x");
    assert!(
        close_degrees(perpendicular_outcome.angle, 72.0),
        "perpendicular DI bends 18 degrees, got {}",
        perpendicular_outcome.angle.to_degrees()
    );
    assert_eq!(neutral_state.velocity, neutral_outcome.velocity);
    assert_eq!(perpendicular_state.velocity, perpendicular_outcome.velocity);
}

#[test]
fn hit_tape_restores_from_serde_and_replays_identically() {
    let rules = rules();
    let hit = strong_hit();
    let mut tape = vec![
        FighterEvent::Input(Input {
            axis: 1.0,
            buttons: 0,
        }),
        FighterEvent::Input(Input {
            axis: 1.0,
            buttons: 0,
        }),
        FighterEvent::Hit(hit),
    ];
    // Tail spans hitlag (inputs ignored), hitstun launch and recovery.
    for tick in 0..30 {
        tape.push(FighterEvent::Input(Input {
            axis: if tick % 3 == 0 { -1.0 } else { 1.0 },
            buttons: if tick % 5 == 0 { game_fighter::button::JUMP } else { 0 },
        }));
    }

    let full = run(&rules, &tape);
    let cut = 3; // immediately after the hit
    let mut restored = {
        let mut state = State::new(&rules);
        for event in &tape[..cut] {
            dispatch(&mut state, *event, &rules);
        }
        let saved = serde_json::to_string(&state).unwrap();
        let loaded: State = serde_json::from_str(&saved).unwrap();
        assert_eq!(loaded, state, "serde round-trip preserves combat state");
        loaded
    };
    for (offset, event) in tape[cut..].iter().enumerate() {
        dispatch(&mut restored, *event, &rules);
        assert_eq!(restored, full[cut + offset], "replay diverged at {offset}");
    }
}
