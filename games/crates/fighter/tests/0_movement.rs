use game_fighter::{Input, Phase, Rules, State, advance, button};

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

fn input(axis: f32, buttons: u8) -> Input {
    Input { axis, buttons }
}

fn squat_to_jump(state: &mut State, rules: &Rules, buttons: u8) {
    advance(state, input(0.0, button::JUMP), rules);
    for _ in 1..rules.jump_startup_time {
        advance(state, input(0.0, buttons), rules);
    }
}

#[test]
fn dash_and_walk_are_mirrored() {
    let rules = rules();
    let mut right = State::new(&rules);
    let mut left = State::new(&rules);
    advance(&mut right, input(1.0, 0), &rules);
    advance(&mut left, input(-1.0, 0), &rules);
    assert_eq!(right.phase, Phase::Dash);
    assert_eq!(left.phase, Phase::Dash);
    assert_eq!(right.velocity[0], -left.velocity[0]);
    assert_eq!(right.position[0], -left.position[0]);

    let mut right = State::new(&rules);
    let mut left = State::new(&rules);
    advance(&mut right, input(0.5, 0), &rules);
    advance(&mut left, input(-0.5, 0), &rules);
    for _ in 0..30 {
        advance(&mut right, input(0.5, 0), &rules);
        advance(&mut left, input(-0.5, 0), &rules);
    }
    assert_eq!(right.velocity[0], -left.velocity[0]);
    assert_eq!(right.position[0], -left.position[0]);
}

#[test]
fn walk_entry_faces_input_and_partial_air_stick_uses_both_attributes() {
    let rules = rules();
    let mut state = State::new(&rules);
    state.facing = -1.0;
    advance(&mut state, input(0.5, 0), &rules);
    assert_eq!((state.phase, state.facing), (Phase::Walk, 1.0));
    state.phase = Phase::Fall;
    state.position[1] = 20.0;
    state.velocity = [0.0, -0.1];
    advance(&mut state, input(0.5, 0), &rules);
    assert_eq!(
        state.velocity[0],
        0.5 * rules.air_drift_stick_mul + rules.air_drift_base
    );
}

#[test]
fn dash_reverse_is_bounded_and_run_stays_below_cap() {
    let rules = rules();
    let mut state = State::new(&rules);
    state.phase = Phase::Dash;
    state.facing = 1.0;
    state.velocity[0] = 4.0;
    advance(&mut state, input(-1.0, 0), &rules);
    assert_eq!(state.velocity[0], 2.0);

    let mut state = State::new(&rules);
    advance(&mut state, input(1.0, 0), &rules);
    for _ in 0..100 {
        advance(&mut state, input(1.0, 0), &rules);
        assert!(state.velocity[0] <= rules.ground_max_horizontal_velocity);
    }
    assert_eq!(state.phase, Phase::Run);
}

#[test]
fn jump_accepts_ground_edges_including_turn_before_reversal() {
    let rules = rules();
    for phase in [
        Phase::Idle,
        Phase::Walk,
        Phase::Dash,
        Phase::Run,
        Phase::Brake,
        Phase::Turn,
        Phase::Crouch,
    ] {
        let mut state = State::new(&rules);
        state.phase = phase;
        state.velocity[0] = if phase == Phase::Brake { 0.5 } else { 0.0 };
        advance(&mut state, input(-1.0, button::JUMP | button::DOWN), &rules);
        assert_eq!(state.phase, Phase::Squat, "{phase:?}");
        assert_eq!(state.phase_tick, 1);
        assert!(!state.short_hop);
    }
}

#[test]
fn jump_lifetime_skips_first_gravity_and_falls_after_apex() {
    let rules = rules();
    let mut state = State::new(&rules);
    squat_to_jump(&mut state, &rules, button::JUMP);
    assert_eq!(state.phase, Phase::Jump);
    assert_eq!(state.velocity[1], rules.jump_v_initial_velocity);
    assert_eq!(state.position[1], rules.jump_v_initial_velocity);
    for _ in 0..30 {
        advance(&mut state, input(0.0, 0), &rules);
    }
    assert_eq!(state.phase, Phase::Fall);
}

#[test]
fn short_hop_double_jump_and_air_facing() {
    let rules = rules();
    let mut state = State::new(&rules);
    squat_to_jump(&mut state, &rules, 0);
    assert!(state.short_hop);
    assert_eq!(state.velocity[1], rules.hop_v_initial_velocity);

    state.phase = Phase::Fall;
    state.position = [0.0, 10.0];
    state.velocity = [0.8, -0.5];
    state.jumps_left = 1;
    state.facing = 1.0;
    advance(&mut state, input(-1.0, button::JUMP), &rules);
    assert_eq!(state.phase, Phase::AirJump);
    assert_eq!(state.jumps_left, 0);
    assert_eq!(state.velocity[0], -rules.air_jump_h_multiplier);
    assert_eq!(state.facing, 1.0);
    advance(&mut state, input(-1.0, 0), &rules);
    assert_eq!(state.facing, 1.0);

    state.phase = Phase::Fall;
    state.position = [0.0, 10.0];
    state.velocity = [2.1, -0.5];
    state.facing = 1.0;
    advance(&mut state, input(1.0, 0), &rules);
    assert!(state.velocity[0] > rules.air_drift_max);
}

#[test]
fn non_finite_input_does_not_mutate_state_and_replay_is_equal() {
    let rules = rules();
    let mut state = State::new(&rules);
    let before = state.clone();
    advance(&mut state, input(f32::NAN, 0), &rules);
    assert_eq!(state, before);

    let mut a = State::new(&rules);
    let tape = (0..240)
        .map(|i| {
            let axis = match i % 40 {
                0..=9 => 1.0,
                10..=19 => 0.0,
                20..=29 => -1.0,
                _ => 0.0,
            };
            input(axis, if i % 60 == 0 { button::JUMP } else { 0 })
        })
        .collect::<Vec<_>>();
    for tick in &tape {
        advance(&mut a, *tick, &rules);
    }
    let checkpoint = a.clone();
    let mut b = checkpoint.clone();
    let mut c = checkpoint;
    for _ in 0..50 {
        advance(&mut b, input(1.0, 0), &rules);
        advance(&mut c, input(1.0, 0), &rules);
    }
    assert_eq!(b, c);
}
