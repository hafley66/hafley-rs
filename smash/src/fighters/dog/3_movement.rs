//! Dog runtime-neutral movement rules.
//!
//! [`catalog`] owns Dog's action identity. This module owns the source-free
//! [`rules`] constructor over the committed generated attribute vocabulary and
//! the explicit local policy. No simulation or animation selection is exposed.

#[path = "1_catalog.rs"]
pub mod catalog;
#[path = "generated/2_attributes.rs"]
mod attr;
#[path = "generated/3_rules.rs"]
mod generated_rules;

/// Source-free locomotion rules for Dog, constructed by the generated artifact
/// from the committed attribute vocabulary and the explicit local policy.
pub fn rules() -> game_fighter::Rules {
    generated_rules::rules()
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
