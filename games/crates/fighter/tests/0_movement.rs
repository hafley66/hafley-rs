//! Exact numeric tests with explicit synthetic rules (no character data).

use game_fighter::{MovementSlice, Phase, Rules, State, button};
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
        jump_h_initial_velocity: 0.5,
        jump_h_max_velocity: 1.2,
        jump_v_initial_velocity: 3.0,
        hop_v_initial_velocity: 2.0,
        ground_to_air_jump_momentum_multiplier: 0.5,
        max_jumps: 1,
        air_jump_v_multiplier: 1.0,
        air_jump_h_multiplier: 0.9,
        gravity: 0.2,
        terminal_velocity: 2.0,
        fast_fall_velocity: 2.8,
        air_drift_stick_mul: 0.06,
        air_drift_max: 1.0,
        aerial_friction: 0.02,
        landing_lag: 4,
    }
}

fn idle(axis: f32, buttons: u8) -> game_fighter::Input {
    game_fighter::Input { buttons, axis }
}

#[test]
fn dash_entry_impulse_from_rest() {
    let r = rules();
    let mut s = State::default();
    s.advance(idle(1.0, 0), &r);
    assert_eq!(s.phase, Phase::Dash);
    assert_eq!(s.velocity[0], 2.0);
    assert_eq!(s.position[0], 2.0);
    assert_eq!(s.facing, 1.0);
}

#[test]
fn dash_reversal_subtracts_current_velocity() {
    let r = rules();
    let mut s = State::default();
    s.velocity[0] = 2.0; // running toward +x, facing +1
    s.facing = 1.0;
    s.phase = Phase::Dash;
    s.advance(idle(-1.0, 0), &r);
    // init = -2.0; gr_vel*facing < 0 so impulse = init - gr_vel = -4.0
    assert_eq!(s.velocity[0], -4.0);
    assert_eq!(s.position[0], -4.0);
    assert_eq!(s.facing, -1.0);
}

#[test]
fn dash_to_run_after_window_and_release_brakes() {
    let r = rules();
    let mut s = State::default();
    // tick 0 enters dash with impulse 2.0, position 2.0
    s.advance(idle(1.0, 0), &r);
    for _ in 0..r.dash_ticks {
        s.advance(idle(1.0, 0), &r);
    }
    assert_eq!(s.phase, Phase::Run);
    assert!(s.velocity[0] <= 1.6);
    // release: run -> brake; brake applies linear traction from the next tick
    s.advance(idle(0.0, 0), &r);
    assert_eq!(s.phase, Phase::Brake);
    assert_eq!(s.velocity[0], 1.6); // transition tick applies no friction
    s.advance(idle(0.0, 0), &r);
    assert_eq!(s.velocity[0], 1.5); // linear traction 0.1
}

#[test]
fn walk_is_analog_and_tapered() {
    let r = rules();
    let mut s = State::default();
    s.advance(idle(0.5, 0), &r);
    assert_eq!(s.phase, Phase::Walk);
    assert_eq!(s.velocity[0], 0.0); // transition tick carries no accel
    s.advance(idle(0.5, 0), &r);
    // accel = 0.1*(1 - 0/0.4) = 0.1
    assert_eq!(s.velocity[0], 0.1);
    assert_eq!(s.position[0], 0.1);
    // approach target 0.4 asymptotically, never overshoot
    for _ in 0..60 {
        s.advance(idle(0.5, 0), &r);
        assert!(s.velocity[0] <= 0.4 + f32::EPSILON);
    }
    assert!((s.velocity[0] - 0.4).abs() < 1e-3);
}

#[test]
fn jumpsquat_release_selects_short_hop() {
    let r = rules();
    let mut s = State::default();
    s.advance(idle(0.0, button::JUMP), &r); // enter squat
    assert_eq!(s.phase, Phase::Squat);
    // release after 2 frames, squat runs 4
    s.advance(idle(0.0, 0), &r);
    s.advance(idle(0.0, 0), &r);
    s.advance(idle(0.0, 0), &r); // phase_tick 3 -> takeoff on this tick
    assert_eq!(s.phase, Phase::Jump);
    assert!(s.short_hop);
    assert_eq!(s.velocity[1], 2.0); // hop_v_initial_velocity
}

#[test]
fn jumpsquat_hold_gives_full_hop() {
    let r = rules();
    let mut s = State::default();
    s.advance(idle(0.0, button::JUMP), &r);
    s.advance(idle(0.0, button::JUMP), &r);
    s.advance(idle(0.0, button::JUMP), &r);
    s.advance(idle(0.0, button::JUMP), &r); // takeoff
    assert_eq!(s.phase, Phase::Jump);
    assert!(!s.short_hop);
    assert_eq!(s.velocity[1], 3.0);
    // gravity applies only from the next tick (Jump -> Fall)
    s.advance(idle(0.0, 0), &r);
    assert_eq!(s.phase, Phase::Fall);
    assert_eq!(s.position[1], 3.0 + 2.8);
}

#[test]
fn gravity_terminal_and_fastfall() {
    let r = rules();
    let mut s = State::default();
    s.phase = Phase::Fall;
    s.position[1] = 100.0;
    s.velocity[1] = 0.0;
    for _ in 0..30 {
        s.advance(idle(0.0, 0), &r);
    }
    assert_eq!(s.velocity[1], -2.0); // terminal
    // fastfall press while falling
    s.velocity[1] = -2.0;
    s.previous_buttons = 0;
    s.advance(idle(0.0, button::DOWN), &r);
    assert!(s.fast_fall);
    // gravity then clamp: -2.0 - 0.2 = -2.2, above the -2.8 fastfall floor
    assert_eq!(s.velocity[1], -2.2);
}

#[test]
fn double_jump_overwrites_velocity() {
    let r = rules();
    let mut s = State::default();
    s.phase = Phase::Fall;
    s.position = [5.0, 10.0];
    s.velocity = [-0.6, -1.5];
    s.jumps_left = 1;
    s.previous_buttons = 0;
    s.advance(idle(1.0, button::JUMP), &r);
    assert_eq!(s.phase, Phase::AirJump);
    assert_eq!(s.velocity[0], 1.0 * 0.9);
    assert_eq!(s.velocity[1], 3.0); // takeoff tick skips gravity
    assert_eq!(s.jumps_left, 0);
}

#[test]
fn landing_resets_jumps_and_locks_lag() {
    let r = rules();
    let mut s = State::default();
    s.phase = Phase::Fall;
    s.position = [0.0, 0.05];
    s.velocity = [0.3, -0.2];
    s.jumps_left = 0;
    s.fast_fall = true;
    s.advance(idle(0.0, 0), &r);
    assert_eq!(s.phase, Phase::Landing);
    assert_eq!(s.jumps_left, 1);
    assert!(!s.fast_fall);
    assert_eq!(s.position[1], 0.0);
    for _ in 0..r.landing_lag {
        s.advance(idle(0.0, 0), &r);
    }
    assert_eq!(s.phase, Phase::Idle);
}

#[test]
fn tape_240_ticks_full_state_restore() {
    let r = rules();
    let mut a = State::default();
    let mut script = Vec::new();
    for i in 0..240 {
        let axis = match i % 40 {
            0..=9 => 1.0,
            10..=19 => 0.0,
            20..=29 => -1.0,
            _ => 0.0,
        };
        let buttons = if i % 60 == 0 || i % 60 == 1 {
            button::JUMP
        } else if i % 90 == 0 {
            button::DOWN
        } else {
            0
        };
        script.push(game_fighter::Input { buttons, axis });
    }
    for inp in &script {
        a.advance(*inp, &r);
    }
    let checkpoint = a.clone();
    // diverge
    let mut b = checkpoint.clone();
    for _ in 0..50 {
        b.advance(idle(1.0, 0), &r);
    }
    assert_ne!(b, checkpoint);
    // restore and replay identical tail from the checkpoint: determinism
    let mut c = checkpoint.clone();
    for _ in 0..50 {
        c.advance(idle(1.0, 0), &r);
    }
    assert_eq!(b, c);
}

#[test]
fn slice_dispatch_matches_state_method() {
    let r = rules();
    let mut s1 = State::default();
    let mut s2 = State::default();
    for i in 0..30 {
        let inp = idle(if i % 2 == 0 { 1.0 } else { 0.0 }, 0);
        MovementSlice::reduce(&mut s1, inp, &r, &mut |_| unreachable!());
        s2.advance(inp, &r);
    }
    assert_eq!(s1, s2);
}

#[test]
fn serialization_roundtrip() {
    let r = rules();
    let mut s = State::default();
    for i in 0..100 {
        s.advance(idle(if i % 3 == 0 { -0.9 } else { 0.5 }, 0), &r);
    }
    let json = serde_json::to_string(&s).unwrap();
    let back: State = serde_json::from_str(&json).unwrap();
    assert_eq!(s, back);
}
