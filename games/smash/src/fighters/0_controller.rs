//! Shared, source-free base locomotion controller for the smash fighter layer.
//!
//! Owns only the mutable presentation facts both characters need: the
//! authoritative [`game_fighter::State`], the selected catalog action id, and
//! its animation frame. Attacks, damage, sandbag state and per-character
//! statecharts stay in the character modules.
//!
//! One tick calls the authoritative [`game_fighter`] reducer once, asks the
//! caller's Phase/axis selector for the exact catalog action, resets the
//! animation on an action or Phase transition, and wraps the frame inside the
//! selected action so it always addresses a real frame. The immutable baked
//! actions and `Rules` stay outside the snapshot.

use serde::{Deserialize, Serialize};

use game_content::Action;
use game_fighter::{Input, Phase, Rules, State};

/// Mutable base locomotion facts. Every field is part of a rollback snapshot;
/// the baked actions and `Rules` are shared context, not state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ControllerState {
    pub fighter: State,
    pub action: usize,
    pub animation: usize,
}

impl ControllerState {
    pub fn new(rules: &Rules) -> Self {
        ControllerState {
            fighter: State::new(rules),
            action: 0,
            animation: 0,
        }
    }
}

/// One fixed tick at the shared controller boundary. `select` is the
/// character's Phase/axis Phase-to-action binding.
#[tracing::instrument(target = "smash::controller", level = "trace", skip_all, fields(buttons = input.buttons, axis = input.axis))]
pub fn advance(
    state: &mut ControllerState,
    input: Input,
    rules: &Rules,
    actions: &[Action],
    select: impl Fn(Phase, f32) -> Option<usize>,
) {
    let previous_phase = state.fighter.phase;
    let previous_action = state.action;
    game_fighter::advance(&mut state.fighter, input, rules);
    if let Some(action) = select(state.fighter.phase, input.axis) {
        let frames = actions[action].frames.len();
        if action != previous_action || state.fighter.phase != previous_phase {
            state.animation = 0;
        } else if frames > 0 {
            state.animation = (state.animation + 1) % frames;
        }
        state.action = action;
    }
}
