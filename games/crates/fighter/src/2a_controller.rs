//! Character-independent locomotion controller over the authoritative reducer.
//!
//! One fixed tick runs the `game_fighter` reducer, then asks the caller's
//! `Phase`/axis selector for the exact action, resetting the animation clock on
//! a phase or action change and advancing it with a safe wrap. Only mutable
//! presentation facts live here: the reducer state, the selected action id, and
//! the animation frame. Immutable baked actions and `Rules` stay caller-owned
//! and never enter a snapshot.
//!
//! The caller supplies the action count and a frame-count lookup, so this crate
//! never depends on the caller's action catalog. Selection is a function
//! parameter, so there is no trait, box, or getter layer.

use crate::{Input, Phase, Rules, State};
use serde::{Deserialize, Serialize};

/// Mutable facts shared by every source-free character: the authoritative
/// reducer state plus the selected action id and its animation frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Controller {
    pub fighter: State,
    pub action: usize,
    pub animation: usize,
}

impl Controller {
    pub fn new(fighter: State) -> Self {
        Controller { fighter, action: 0, animation: 0 }
    }

    /// Animation frame for the selected action, clamped to its frame count.
    pub fn frame(&self, frame_count: impl Fn(usize) -> usize) -> usize {
        let frames = frame_count(self.action);
        if frames == 0 { 0 } else { self.animation.min(frames - 1) }
    }

    /// One fixed tick: reducer first, then selection for the resulting phase and
    /// axis. A phase or action change resets the animation; otherwise the clock
    /// advances and wraps inside the action's frame count.
    pub fn advance(
        &mut self,
        input: Input,
        rules: &Rules,
        action_count: usize,
        frame_count: impl Fn(usize) -> usize,
        select: impl Fn(Phase, f32) -> Option<usize>,
    ) {
        if action_count == 0 {
            return;
        }
        let previous = self.fighter.phase;
        crate::advance(&mut self.fighter, input, rules);
        let phase = self.fighter.phase;
        let selected = select(phase, input.axis)
            .filter(|action| *action < action_count)
            .unwrap_or(self.action.min(action_count - 1));
        let frames = frame_count(selected);
        if selected != self.action || phase != previous || frames == 0 {
            self.animation = 0;
        } else {
            self.animation = (self.animation + 1) % frames;
        }
        self.action = selected;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn input(axis: f32, buttons: u8) -> Input {
        Input { axis, buttons }
    }

    #[test]
    fn frame_clamps_to_action_length() {
        let controller = Controller { fighter: State::new(&rules()), action: 0, animation: 99 };
        assert_eq!(controller.frame(|_| 4), 3);
        assert_eq!(controller.frame(|_| 0), 0);
    }

    #[test]
    fn action_or_phase_change_resets_and_advance_wraps() {
        let rules = rules();
        let mut controller = Controller::new(State::new(&rules));
        let frames = |_| 3;

        controller.advance(input(0.0, 0), &rules, 2, frames, |_, _| Some(0));
        assert_eq!((controller.action, controller.animation), (0, 1));
        controller.advance(input(0.0, 0), &rules, 2, frames, |_, _| Some(0));
        assert_eq!((controller.action, controller.animation), (0, 2));
        controller.advance(input(0.0, 0), &rules, 2, frames, |_, _| Some(0));
        assert_eq!((controller.action, controller.animation), (0, 0));
        controller.advance(input(0.0, 0), &rules, 2, frames, |_, _| Some(0));
        assert_eq!((controller.action, controller.animation), (0, 1));

        controller.advance(input(0.0, 0), &rules, 2, frames, |_, _| Some(1));
        assert_eq!((controller.action, controller.animation), (1, 0));
    }

    #[test]
    fn empty_catalog_is_a_no_op() {
        let rules = rules();
        let mut controller = Controller::new(State::new(&rules));
        let before = controller.clone();
        controller.advance(input(0.5, 0), &rules, 0, |_| 0, |_, _| Some(0));
        assert_eq!(controller, before);
    }

    #[test]
    fn out_of_range_selection_falls_back_to_clamped_action() {
        let rules = rules();
        let mut controller = Controller::new(State::new(&rules));
        controller.action = 9;
        controller.advance(input(0.0, 0), &rules, 2, |_| 2, |_, _| Some(7));
        assert_eq!(controller.action, 1);
        assert_eq!(controller.animation, 0);
    }
}
