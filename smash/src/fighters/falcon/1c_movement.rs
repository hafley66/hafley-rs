//! Live locomotion using imported PM attributes and shared fighter phases.
use super::{Action, World};
use game_fighter::{Input, Phase, Rules, State};

#[path = "generated/1_attributes.rs"]
mod attr;

pub fn rules() -> Rules {
    Rules {
        walk_init_vel: attr::WALK_INIT_VEL, walk_accel: attr::WALK_ACC,
        walk_max_vel: attr::WALK_MAX_VEL, walk_stick_threshold: 0.2,
        dash_stick_threshold: 0.8, dash_initial_velocity: attr::DASH_INIT_VEL,
        dash_accel_base: attr::DASH_RUN_ACC_B, dash_accel_mul: attr::DASH_RUN_ACC_A,
        dash_max_velocity: attr::DASH_RUN_TERM_VEL,
        // Common transition policy, pending imported common callback timings.
        dash_ticks: 15, ground_friction: attr::GROUND_FRICTION,
        dash_friction_mul: 1.0, ground_max_horizontal_velocity: attr::GROUNDED_MAX_X_VEL,
        turn_ticks: attr::FLIP_DIR_FRAME as u32,
        jump_startup_time: attr::JUMP_SQUAT_FRAMES as u32,
        jump_h_initial_velocity: attr::JUMP_X_INIT_VEL,
        jump_h_max_velocity: attr::JUMP_X_INIT_TERM_VEL,
        jump_v_initial_velocity: attr::JUMP_Y_INIT_VEL,
        hop_v_initial_velocity: attr::JUMP_Y_INIT_VEL_SHORT,
        ground_to_air_jump_momentum_multiplier: attr::JUMP_X_VEL_GROUND_MULT,
        max_jumps: attr::NUM_JUMPS as u8,
        air_jump_v_multiplier: attr::AIR_JUMP_Y_MULT,
        air_jump_h_multiplier: attr::AIR_JUMP_X_MULT,
        gravity: attr::GRAVITY, terminal_velocity: attr::TERM_VEL,
        fast_fall_velocity: attr::FASTFALL_VELOCITY,
        air_drift_stick_mul: attr::AIR_MOBILITY_A,
        air_drift_base: attr::AIR_MOBILITY_B,
        air_drift_max: attr::AIR_X_TERM_VEL, aerial_friction: attr::AIR_FRICTION_X,
        landing_lag: attr::NORMAL_LANDING_LAG as u32,
    }
}

pub fn initial() -> State {
    State { position: [-12.0, 0.0], ..State::new(&rules()) }
}

pub fn advance(world: &mut World, buttons: u8, axis: f32, actions: &[Action]) -> [f32; 3] {
    let mut policy = rules();
    let fighter = world.movement.as_mut().unwrap();
    let old_phase = fighter.phase;
    let attacking = world.action == 2 && world.animation < actions[2].frames.len();
    let recovering = world.action == 5 && world.animation < actions[5].frames.len();
    if recovering { policy.landing_lag = actions[5].frames.len() as u32; }
    let input = Input { buttons: if attacking { buttons & !1 } else { buttons }, axis };
    game_fighter::advance(fighter, input, &policy);
    let landed = !old_phase.grounded() && fighter.phase == Phase::Landing;
    let mut action = match fighter.phase {
        Phase::Idle => 0,
        Phase::Walk => if axis.abs() < 0.35 { 7 } else if axis.abs() < 0.65 { 8 } else { 9 },
        Phase::Dash => 10, Phase::Run => 11, Phase::Brake => 12, Phase::Turn => 14,
        Phase::Squat => 3, Phase::Jump => 1, Phase::Fall => 4,
        Phase::AirJump => 16, Phase::Crouch => 3, Phase::Landing => 6,
    };
    if attacking && !fighter.grounded() { action = 2; }
    if (landed && attacking && actions[2].frames[world.animation.min(actions[2].frames.len()-1)].landing_lag)
        || (recovering && fighter.phase == Phase::Landing) { action = 5; }
    let pressed = buttons & !world.previous_input;
    if !fighter.grounded() && !attacking && pressed & 2 != 0 {
        action = 2;
        world.attack_hit = false;
    }
    if action != world.action || old_phase != fighter.phase {
        // An aerial attack's pose clock continues across the jump apex.
        if action != 2 || world.action != 2 { world.animation = 0; }
        world.action = action;
    }
    if matches!(action, 0 | 4 | 7..=11) {
        world.animation %= actions[action].frames.len();
    }
    if fighter.phase == Phase::Crouch { world.animation = actions[3].frames.len() - 1; }
    [0.0, fighter.position[1], fighter.position[0]]
}

#[cfg(all(test, feature = "ingest"))]
mod tests {
    use super::*;
    use super::super::{catalog, Simulation};
    use std::sync::Arc;

    fn assets() -> Arc<[Action]> { game_content::bake(&catalog::load().unwrap()).into() }

    #[test]
    fn imported_dash_run_and_full_animation_cycle_are_used() {
        let data = assets();
        let mut right = Simulation::new_locomotion(data.clone(), false);
        let mut left = Simulation::new_locomotion(data, false);
        let mut run_poses = std::collections::BTreeSet::new();
        for tick in 0..90 {
            let r = right.advance_controlled(0, 1.0);
            let l = left.advance_controlled(0, -1.0);
            assert!((r.view.root[2] + l.view.root[2] + 24.0).abs() < 0.0001);
            if tick == 0 { assert_eq!((r.action, r.view.root[2]), (10, -10.0)); }
            if r.action == 11 { run_poses.insert(r.view.frame); }
        }
        assert_eq!(run_poses.into_iter().collect::<Vec<_>>(), (0..21).collect::<Vec<_>>());
        assert!((right.state().movement.as_ref().unwrap().velocity[0] - 2.3).abs() < 0.0001);
    }

    #[test]
    fn live_falcon_restores_complete_state_across_movement_and_fair() {
        let data = assets();
        let tape: Vec<_> = (0..360).map(|t| {
            let buttons = match t { 35..=42 | 150..=156 | 270 => 1, 55 | 175 => 2, 220..=225 => 4, _ => 0 };
            let axis = match t { 0..=75 => 1.0, 100..=180 => -1.0, 240..=300 => 0.4, _ => 0.0 };
            (buttons, axis)
        }).collect();
        let mut sim = Simulation::new_locomotion(data.clone(), false);
        let mut states = Vec::new();
        let mut saves = Vec::new();
        for (tick, &(buttons, axis)) in tape.iter().enumerate() {
            if tick % 27 == 0 { saves.push((tick, sim.save())); }
            states.push(sim.advance_controlled(buttons, axis).clone());
        }
        for (tick, snapshot) in saves {
            sim.load(&snapshot);
            for (offset, &(buttons, axis)) in tape[tick..].iter().enumerate() {
                assert_eq!(sim.advance_controlled(buttons, axis), &states[tick + offset]);
            }
        }
        for state in states.iter().step_by(17) {
            let restored: World = serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
            assert_eq!(&restored, state);
        }
        assert!(states.iter().any(|s| s.action == 2));
        assert!(states.iter().any(|s| s.action == 11));
    }

    #[test]
    fn turn_jump_interrupt_selects_jump_squat_and_replays_exactly() {
        let mut sim = Simulation::new_locomotion(assets(), false);
        for _ in 0..40 { sim.advance_controlled(0, 1.0); }
        assert_eq!(sim.state().movement.as_ref().unwrap().phase, Phase::Run);
        sim.advance_controlled(0, -1.0);
        assert_eq!(sim.state().movement.as_ref().unwrap().phase, Phase::Turn);
        let turn = sim.save();
        let jumped = sim.advance_controlled(1, -1.0).clone();
        assert_eq!(jumped.movement.as_ref().unwrap().phase, Phase::Squat);
        // View carries the emitted pose; the world clock already points at
        // the next animation frame after publication.
        assert_eq!((jumped.action, jumped.view.frame, jumped.animation), (3, 0, 1));
        sim.load(&turn);
        assert_eq!(sim.advance_controlled(1, -1.0), &jumped);
    }
}
