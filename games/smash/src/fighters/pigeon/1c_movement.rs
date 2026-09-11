//! Live locomotion using imported PM attributes and shared fighter phases.
use super::{Action, World};
use game_fighter::{Input, Phase, Rules, State};

#[path = "generated/1_attributes.rs"]
mod attr;
#[path = "generated/7_rules.rs"]
mod generated_rules;
#[path = "generated/8_roles.rs"]
mod roles;

/// Live locomotion rules for Pigeon, constructed by the generated artifact.
/// Attribute values come from the generated attribute vocabulary; explicit
/// local policy lives in the generator. Runtime overrides for crouch and
/// landing lag remain in [`advance`].
pub fn rules() -> Rules {
    generated_rules::rules()
}

pub fn initial() -> State {
    State { position: [-12.0, 0.0], ..State::new(&rules()) }
}

/// Pure host Phase -> catalog animation seam for the base pose. Names come from
/// the generated role bindings; this module authors no numeric catalog ID.
/// Conditional live selections (air attack, aerial-landing recovery) go through
/// [`select`].
pub fn pose_for_phase(phase: Phase, axis: f32) -> usize {
    match phase {
        Phase::Idle => roles::IDLE.expect("Pigeon Idle role"),
        Phase::Walk => if axis.abs() < 0.35 {
            roles::WALK_SLOW.expect("Pigeon WalkSlow role")
        } else if axis.abs() < 0.65 {
            roles::WALK_MIDDLE.expect("Pigeon WalkMiddle role")
        } else {
            roles::WALK_FAST.expect("Pigeon WalkFast role")
        },
        Phase::Dash => roles::DASH.expect("Pigeon Dash role"),
        Phase::Run => roles::RUN.expect("Pigeon Run role"),
        Phase::Brake => roles::BRAKE.expect("Pigeon Brake role"),
        Phase::Turn => roles::TURN.expect("Pigeon Turn role"),
        Phase::Squat => roles::JUMP_SQUAT.expect("Pigeon JumpSquat role"),
        Phase::CrouchEnter => roles::CROUCH_ENTER.expect("Pigeon CrouchEnter role"),
        Phase::CrouchHold => roles::CROUCH_HOLD.expect("Pigeon CrouchHold role"),
        Phase::CrouchExit => roles::CROUCH_EXIT.expect("Pigeon CrouchExit role"),
        Phase::Jump => roles::JUMP.expect("Pigeon Jump role"),
        Phase::Fall => roles::FALL.expect("Pigeon Fall role"),
        Phase::AirJump => roles::AIR_JUMP.expect("Pigeon AirJump role"),
        Phase::Landing => roles::LANDING.expect("Pigeon Landing role"),
    }
}

/// Caller-resolved facts that choose a catalog animation for one tick. These
/// are the exact runtime inputs the shipped controller resolves before the
/// Phase-to-action seam; the export evaluates the same function over them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectionFacts {
    /// Aerial attack animation is still playing.
    pub attacking: bool,
    /// Attack button was pressed this tick.
    pub attack_pressed: bool,
    /// The fighter just contacted the ground and entered `Landing`.
    pub landed: bool,
    /// Aerial landing-recovery animation is still playing.
    pub recovering: bool,
    /// The aerial attack's landing-lag fact holds for this frame.
    pub landing_lag: bool,
}

/// Machine key for the rule that won a selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condition {
    Base,
    AirAttack,
    LandingRecovery,
}

impl Condition {
    pub fn key(self) -> &'static str {
        match self {
            Condition::Base => "base",
            Condition::AirAttack => "air_attack",
            Condition::LandingRecovery => "landing_recovery",
        }
    }
}

/// Pure Phase-to-action selection shared by the shipped controller and the
/// status export. Precedence mirrors the controller exactly: base pose, then
/// the airborne attack override, then aerial-landing recovery, then a fresh
/// airborne attack press.
pub fn select(phase: Phase, axis: f32, facts: SelectionFacts) -> (usize, Condition) {
    let air_attack = roles::AIR_ATTACK.expect("Pigeon AirAttack role");
    let landing_recovery = roles::LANDING_RECOVERY.expect("Pigeon LandingRecovery role");
    let mut action = pose_for_phase(phase, axis);
    let mut condition = Condition::Base;
    if facts.attacking && !phase.grounded() {
        action = air_attack;
        condition = Condition::AirAttack;
    }
    if (facts.landed && phase == Phase::Landing && facts.attacking && facts.landing_lag)
        || (facts.recovering && phase == Phase::Landing)
    {
        action = landing_recovery;
        condition = Condition::LandingRecovery;
    }
    if !phase.grounded() && !facts.attacking && facts.attack_pressed {
        action = air_attack;
        condition = Condition::AirAttack;
    }
    (action, condition)
}

pub fn advance(world: &mut World, buttons: u8, axis: f32, actions: &[Action]) -> [f32; 3] {
    let air_attack = roles::AIR_ATTACK.expect("Pigeon AirAttack role");
    let landing_recovery = roles::LANDING_RECOVERY.expect("Pigeon LandingRecovery role");
    let crouch_enter = roles::CROUCH_ENTER.expect("Pigeon CrouchEnter role");
    let crouch_exit = roles::CROUCH_EXIT.expect("Pigeon CrouchExit role");
    let mut policy = rules();
    let fighter = world.movement.as_mut().unwrap();
    let old_phase = fighter.phase;
    let attacking = world.action == air_attack && world.animation < actions[air_attack].frames.len();
    let recovering = world.action == landing_recovery
        && world.animation < actions[landing_recovery].frames.len();
    if recovering { policy.landing_lag = actions[landing_recovery].frames.len() as u32; }
    // Crouch lifecycle completion is animation-driven; supply the imported lengths.
    policy.crouch_enter_ticks = actions[crouch_enter].frames.len() as u32;
    policy.crouch_exit_ticks = actions[crouch_exit].frames.len() as u32;
    let input = Input { buttons: if attacking { buttons & !1 } else { buttons }, axis };
    game_fighter::advance(fighter, input, &policy);
    let landed = !old_phase.grounded() && fighter.phase == Phase::Landing;
    let pressed = buttons & !world.previous_input;
    let landing_lag = landed
        && attacking
        && actions[air_attack].frames[world.animation.min(actions[air_attack].frames.len()-1)].landing_lag;
    let (action, _) = select(fighter.phase, axis, SelectionFacts {
        attacking,
        attack_pressed: pressed & 2 != 0,
        landed,
        recovering,
        landing_lag,
    });
    if !fighter.grounded() && !attacking && pressed & 2 != 0 {
        world.attack_hit = false;
    }
    if action != world.action || old_phase != fighter.phase {
        // An aerial attack's pose clock continues across the jump apex.
        if action != air_attack || world.action != air_attack { world.animation = 0; }
        world.action = action;
    }
    let looping = [
        roles::IDLE, roles::FALL, roles::WALK_SLOW, roles::WALK_MIDDLE, roles::WALK_FAST,
        roles::DASH, roles::RUN, roles::CROUCH_HOLD,
    ];
    if looping.contains(&Some(action)) {
        world.animation %= actions[action].frames.len();
    }
    [0.0, fighter.position[1], fighter.position[0]]
}

#[cfg(test)]
mod selection_tests {
    use super::*;

    #[test]
    fn selection_surfaces_air_attack_and_landing_recovery_ids() {
        let base = SelectionFacts::default();
        assert_eq!(select(Phase::Fall, 0.0, base), (4, Condition::Base));
        assert_eq!(
            select(Phase::Fall, 0.0, SelectionFacts { attacking: true, ..base }),
            (2, Condition::AirAttack),
        );
        assert_eq!(
            select(Phase::Fall, 0.0, SelectionFacts { attack_pressed: true, ..base }),
            (2, Condition::AirAttack),
        );
        assert_eq!(
            select(Phase::Jump, 0.0, SelectionFacts { attacking: true, ..base }),
            (2, Condition::AirAttack),
        );
        assert_eq!(
            select(Phase::Landing, 0.0, SelectionFacts { landed: true, attacking: true, landing_lag: true, ..base }),
            (5, Condition::LandingRecovery),
        );
        assert_eq!(
            select(Phase::Landing, 0.0, SelectionFacts { recovering: true, ..base }),
            (5, Condition::LandingRecovery),
        );
        assert_eq!(
            select(Phase::Idle, 0.0, SelectionFacts { landed: true, attacking: true, landing_lag: true, ..base }),
            (0, Condition::Base),
            "landing recovery is impossible outside Landing",
        );
    }

    #[test]
    fn selection_maps_crouch_lifecycle_to_squat_ids_and_keeps_jump_squat_pose() {
        let base = SelectionFacts::default();
        // JumpSquat ID3 stays the jump startup pose.
        assert_eq!(select(Phase::Squat, 0.0, base), (3, Condition::Base));
        assert_eq!(select(Phase::CrouchEnter, 0.0, base), (18, Condition::Base));
        assert_eq!(select(Phase::CrouchHold, 0.0, base), (19, Condition::Base));
        assert_eq!(select(Phase::CrouchExit, 0.0, base), (20, Condition::Base));
        // A held crouch must not fall back to the jumpsquat pose.
        assert_ne!(select(Phase::CrouchHold, 0.0, base).0, 3);
    }
}

#[cfg(test)]
mod rules_tests {
    use super::*;

    /// The generated constructor reproduces the retired authored mapping
    /// exactly, attribute for attribute and policy value for policy value.
    #[test]
    fn generated_rules_preserve_the_pigeon_mapping_exactly() {
        let expected = Rules {
            walk_init_vel: attr::WALK_INIT_VEL,
            walk_accel: attr::WALK_ACC,
            walk_max_vel: attr::WALK_MAX_VEL,
            walk_stick_threshold: 0.2,
            dash_stick_threshold: 0.8,
            dash_initial_velocity: attr::DASH_INIT_VEL,
            dash_accel_base: attr::DASH_RUN_ACC_B,
            dash_accel_mul: attr::DASH_RUN_ACC_A,
            dash_max_velocity: attr::DASH_RUN_TERM_VEL,
            dash_ticks: 15,
            ground_friction: attr::GROUND_FRICTION,
            dash_friction_mul: 1.0,
            ground_max_horizontal_velocity: attr::GROUNDED_MAX_X_VEL,
            turn_ticks: attr::FLIP_DIR_FRAME as u32,
            jump_startup_time: attr::JUMP_SQUAT_FRAMES as u32,
            crouch_enter_ticks: 1,
            crouch_exit_ticks: 1,
            jump_h_initial_velocity: attr::JUMP_X_INIT_VEL,
            jump_h_max_velocity: attr::JUMP_X_INIT_TERM_VEL,
            jump_v_initial_velocity: attr::JUMP_Y_INIT_VEL,
            hop_v_initial_velocity: attr::JUMP_Y_INIT_VEL_SHORT,
            ground_to_air_jump_momentum_multiplier: attr::JUMP_X_VEL_GROUND_MULT,
            max_jumps: attr::NUM_JUMPS as u8,
            air_jump_v_multiplier: attr::AIR_JUMP_Y_MULT,
            air_jump_h_multiplier: attr::AIR_JUMP_X_MULT,
            gravity: attr::GRAVITY,
            terminal_velocity: attr::TERM_VEL,
            fast_fall_velocity: attr::FASTFALL_VELOCITY,
            air_drift_stick_mul: attr::AIR_MOBILITY_A,
            air_drift_base: attr::AIR_MOBILITY_B,
            air_drift_max: attr::AIR_X_TERM_VEL,
            aerial_friction: attr::AIR_FRICTION_X,
            landing_lag: attr::NORMAL_LANDING_LAG as u32,
        };
        assert_eq!(rules(), expected);
    }
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
    fn live_pigeon_restores_complete_state_across_movement_and_fair() {
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
    fn live_crouch_lifecycle_selects_squat_ids_and_replays_exactly() {
        let data = assets();
        let crouch_enter_frames = data[18].frames.len();
        let crouch_exit_frames = data[20].frames.len();
        let tape: Vec<(u8, f32)> = (0..40)
            .map(|t| if t < 20 { (4, 0.0) } else { (0, 0.0) })
            .collect();
        let mut sim = Simulation::new_locomotion(data, false);
        let start = sim.save();
        let mut states = Vec::new();
        for &(buttons, axis) in &tape {
            states.push(sim.advance_controlled(buttons, axis).clone());
        }
        let actions: Vec<usize> = states.iter().map(|state| state.action).collect();
        let enter = actions.iter().position(|&a| a == 18).expect("enters Squat");
        let hold = actions.iter().position(|&a| a == 19).expect("holds SquatWait");
        let exit = actions.iter().position(|&a| a == 20).expect("exits SquatRv");
        assert!(enter < hold && hold < exit, "crouch order {actions:?}");
        assert_eq!(actions.last(), Some(&0), "returns to Wait");
        assert!(
            actions.iter().filter(|&&a| a == 19).count() > 1,
            "holds SquatWait across ticks: {actions:?}",
        );
        assert_eq!(
            actions.iter().filter(|&&a| a == 18).count(),
            crouch_enter_frames,
            "Squat residence follows the imported animation length",
        );
        assert_eq!(
            actions.iter().filter(|&&a| a == 20).count(),
            crouch_exit_frames,
            "SquatRv residence follows the imported animation length",
        );
        sim.load(&start);
        for (offset, &(buttons, axis)) in tape.iter().enumerate() {
            assert_eq!(sim.advance_controlled(buttons, axis), &states[offset]);
        }
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
