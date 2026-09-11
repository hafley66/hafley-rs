//! Dog runtime-neutral movement rules.
//!
//! [`catalog`] owns Dog's action identity. This module owns the source-free
//! [`rules`] constructor over the committed generated attribute vocabulary and
//! the explicit local policy, plus a named Phase-to-animation [`select`] over
//! Dog's generated role bindings.

#[path = "1_catalog.rs"]
pub mod catalog;
#[path = "generated/2_attributes.rs"]
mod attr;
#[path = "generated/3_rules.rs"]
mod generated_rules;
#[path = "generated/4_roles.rs"]
mod roles;
#[path = "4_simulation.rs"]
pub mod simulation;

pub use simulation::{Simulation, Snapshot};

use game_fighter::Phase;

/// Source-free locomotion rules for Dog, constructed by the generated artifact
/// from the committed attribute vocabulary and the explicit local policy.
pub fn rules() -> game_fighter::Rules {
    generated_rules::rules()
}

/// Named Phase-to-animation selection over Dog's generated role bindings.
///
/// Every `Phase` resolves to an exact retained Dog clip; no substitute or
/// fallback is claimed. The Walk bands mirror Pigeon's stick policy.
pub fn select(phase: Phase, axis: f32) -> Option<usize> {
    match phase {
        Phase::Idle => roles::IDLE,
        Phase::Walk => if axis.abs() < 0.35 {
            roles::WALK_SLOW
        } else if axis.abs() < 0.65 {
            roles::WALK_MIDDLE
        } else {
            roles::WALK_FAST
        },
        Phase::Dash => roles::DASH,
        Phase::Run => roles::RUN,
        Phase::Brake => roles::BRAKE,
        Phase::Turn => roles::TURN,
        Phase::Squat => roles::JUMP_SQUAT,
        Phase::CrouchEnter => roles::CROUCH_ENTER,
        Phase::CrouchHold => roles::CROUCH_HOLD,
        Phase::CrouchExit => roles::CROUCH_EXIT,
        Phase::Jump => roles::JUMP,
        Phase::Fall => roles::FALL,
        Phase::AirJump => roles::AIR_JUMP,
        Phase::Landing => roles::LANDING,
    }
}

#[cfg(test)]
mod rules_tests {
    use super::*;

    /// Dog's generated constructor reads its own retained attribute values and
    /// the explicit policy shared with the current Pigeon cut.
    #[test]
    fn dog_rules_use_retained_attribute_and_policy_values() {
        let rules = rules();
        assert_eq!(rules.walk_init_vel, 0.3);
        assert_eq!(rules.walk_accel, 0.1);
        assert_eq!(rules.walk_max_vel, 1.3);
        assert_eq!(rules.dash_initial_velocity, 2.05);
        assert_eq!(rules.dash_accel_base, 0.02);
        assert_eq!(rules.dash_accel_mul, 0.15);
        assert_eq!(rules.dash_max_velocity, 1.85);
        assert_eq!(rules.ground_friction, 0.095);
        assert_eq!(rules.gravity, 0.125);
        assert_eq!(rules.terminal_velocity, 2.0);
        assert_eq!(rules.fast_fall_velocity, 2.5);
        assert_eq!(rules.max_jumps, 2);
        assert_eq!(rules.turn_ticks, 4);
        assert_eq!(rules.jump_startup_time, 3);
        assert_eq!(rules.landing_lag, 3);
        assert_eq!(rules.walk_stick_threshold, 0.2);
        assert_eq!(rules.dash_stick_threshold, 0.8);
        assert_eq!(rules.dash_ticks, 15);
        assert_eq!(rules.dash_friction_mul, 1.0);
        assert_eq!(rules.crouch_enter_ticks, 1);
        assert_eq!(rules.crouch_exit_ticks, 1);
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;

    /// Every `game_fighter::Phase` resolves to an exact retained Dog catalog ID.
    #[test]
    fn select_binds_every_phase_to_an_exact_retained_role() {
        for phase in Phase::ALL {
            assert!(select(phase, 0.0).is_some(), "{phase:?} has no bound clip");
        }
        assert_eq!(select(Phase::Idle, 0.0), Some(0));
        assert_eq!(select(Phase::Dash, 0.0), Some(1));
        assert_eq!(select(Phase::Run, 0.0), Some(2));
        assert_eq!(select(Phase::Jump, 0.0), Some(3));
        assert_eq!(select(Phase::AirJump, 0.0), Some(4));
        assert_eq!(select(Phase::CrouchEnter, 0.0), Some(5));
        assert_eq!(select(Phase::CrouchHold, 0.0), Some(6));
        assert_eq!(select(Phase::CrouchExit, 0.0), Some(7));
        assert_eq!(select(Phase::Landing, 0.0), Some(8));
        assert_eq!(select(Phase::Walk, 0.0), Some(16));
        assert_eq!(select(Phase::Walk, 0.5), Some(17));
        assert_eq!(select(Phase::Walk, 0.9), Some(18));
        assert_eq!(select(Phase::Brake, 0.0), Some(19));
        assert_eq!(select(Phase::Turn, 0.0), Some(20));
        assert_eq!(select(Phase::Squat, 0.0), Some(21));
        assert_eq!(select(Phase::Fall, 0.0), Some(22));
        assert_eq!(roles::AIR_ATTACK, Some(12));
        assert_eq!(roles::LANDING_LIGHT, Some(23));
        assert_eq!(roles::LANDING_RECOVERY, Some(24));
    }
}
