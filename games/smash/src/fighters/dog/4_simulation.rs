//! Source-free deterministic Dog runtime over the shared controller.
//!
//! Every later tick is a pure function of the committed baked JSON, the
//! generated `Rules` and role bindings, the shared
//! [`crate::fighters::controller`], and the input tape. No ingest feature,
//! filesystem read or physics formula is introduced here.

use std::sync::Arc;

use game_content::Action;
use game_fighter::{Input, Rules};
use serde::{Deserialize, Serialize};

use crate::fighters::controller::{self, ControllerState};

use super::{catalog, rules, select};

/// A durable copy of the mutable controller state. Immutable baked actions and
/// `Rules` are shared context and never enter the snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot(ControllerState);

/// Source-free Dog simulation: committed baked actions plus generated rules and
/// the shared controller.
pub struct Simulation {
    actions: Arc<[Action]>,
    rules: Rules,
    state: ControllerState,
}

impl Simulation {
    /// Load the embedded committed bake. Fails only if the committed JSON is
    /// malformed; it never opens a payload or touches the filesystem.
    pub fn new() -> Result<Self, serde_json::Error> {
        let actions: Arc<[Action]> = catalog::load_baked()?.into();
        Ok(Self::with_actions(actions))
    }

    pub fn with_actions(actions: Arc<[Action]>) -> Self {
        let rules = rules();
        let state = ControllerState::new(&rules);
        Self { actions, rules, state }
    }

    /// One fixed tick through the shared controller.
    #[tracing::instrument(target = "dog::simulation", level = "trace", skip_all)]
    pub fn advance(&mut self, input: Input) -> &ControllerState {
        controller::advance(&mut self.state, input, &self.rules, &self.actions, select);
        &self.state
    }

    pub fn state(&self) -> &ControllerState {
        &self.state
    }

    pub fn actions(&self) -> &[Action] {
        &self.actions
    }

    #[tracing::instrument(target = "dog::snapshot", level = "trace", skip_all)]
    pub fn save(&self) -> Snapshot {
        Snapshot(self.state.clone())
    }

    pub fn load(&mut self, snapshot: &Snapshot) {
        self.state = snapshot.0.clone();
    }
}

/// Deterministic input tape for the scoped Dog locomotion slice. Each segment
/// names the phase it is intended to reach; the runtime tests report any phase
/// or action the tape does not actually select instead of tuning the tape to a
/// hoped-for observation. Buttons: bit 1 jump, bit 2 attack, bit 4 down.
pub fn tape() -> Vec<(u8, f32)> {
    fn run(tape: &mut Vec<(u8, f32)>, buttons: u8, axis: f32, ticks: usize) {
        for _ in 0..ticks {
            tape.push((buttons, axis));
        }
    }
    let mut tape = Vec::new();
    run(&mut tape, 0, 0.0, 2); // Idle settle
    run(&mut tape, 0, 0.3, 3); // Walk / WalkSlow
    run(&mut tape, 0, 0.5, 3); // Walk / WalkMiddle
    run(&mut tape, 0, 0.7, 3); // Walk / WalkFast
    run(&mut tape, 0, 0.0, 2); // Walk -> Idle
    run(&mut tape, 0, 1.0, 2); // Idle -> Dash
    run(&mut tape, 0, -1.0, 2); // dash reversal (Dash self-transition)
    run(&mut tape, 0, 1.0, 1); // dash dance back
    run(&mut tape, 0, 1.0, 22); // Dash -> Run
    run(&mut tape, 0, -1.0, 1); // Run -> Turn
    run(&mut tape, 0, -1.0, 1); // Turn -> Dash
    run(&mut tape, 0, 0.0, 30); // Dash -> Brake -> Idle
    run(&mut tape, 4, 0.0, 1); // Idle -> CrouchEnter
    run(&mut tape, 4, 0.0, 4); // CrouchEnter -> CrouchHold
    run(&mut tape, 0, 0.0, 1); // CrouchHold -> CrouchExit
    run(&mut tape, 0, 0.0, 2); // CrouchExit -> Idle
    run(&mut tape, 1, 0.0, 1); // Idle -> Squat (jumpsquat)
    run(&mut tape, 0, 0.0, 5); // Squat -> Jump
    run(&mut tape, 0, 0.0, 30); // Jump -> Fall
    run(&mut tape, 1, 0.0, 1); // Fall -> AirJump
    run(&mut tape, 0, 0.0, 90); // AirJump -> Fall -> Landing -> Idle
    tape
}

#[cfg(test)]
#[path = "5_runtime_tests.rs"]
mod runtime_tests;
