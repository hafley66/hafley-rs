//! Shared source-free locomotion controller for the Smash fighter slice.
//!
//! One fixed tick runs the authoritative `game_fighter` reducer, then asks the
//! caller's `Phase`/axis selector for the exact action, resetting the animation
//! clock on a phase or action change and advancing it with a safe wrap. Only
//! mutable presentation facts live here: the reducer state, the selected action
//! id, and the animation frame. Immutable baked actions and `Rules` stay
//! caller-owned and never enter a snapshot.
//!
//! The type is character-independent. Callers pass the selector as a function
//! parameter, so there is no trait, box or getter layer.

use game_content::Action;
use game_fighter::{Input, Phase, Rules, State};
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
    pub fn frame(&self, actions: &[Action]) -> usize {
        let frames = actions.get(self.action).map_or(0, |action| action.frames.len());
        if frames == 0 { 0 } else { self.animation.min(frames - 1) }
    }

    /// One fixed tick: reducer first, then selection for the resulting phase and
    /// axis. A phase or action change resets the animation; otherwise the clock
    /// advances and wraps inside the action's frame count.
    pub fn advance(
        &mut self,
        input: Input,
        rules: &Rules,
        actions: &[Action],
        select: impl Fn(Phase, f32) -> Option<usize>,
    ) {
        if actions.is_empty() {
            return;
        }
        let previous = self.fighter.phase;
        game_fighter::advance(&mut self.fighter, input, rules);
        let phase = self.fighter.phase;
        let selected = select(phase, input.axis)
            .filter(|action| *action < actions.len())
            .unwrap_or(self.action.min(actions.len() - 1));
        let frames = actions[selected].frames.len();
        if selected != self.action || phase != previous || frames == 0 {
            self.animation = 0;
        } else {
            self.animation = (self.animation + 1) % frames;
        }
        self.action = selected;
    }
}
