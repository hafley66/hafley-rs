//! Dog source-free runtime slice.
//!
//! [`Simulation`] owns immutable baked actions and generated [`Rules`] plus the
//! shared [`Controller`]. It reads only committed JSON embedded at build time:
//! no `ingest` feature, no filesystem, no decoder. A [`Snapshot`] carries every
//! mutable fact; the baked actions and rules stay outside it.

use super::{catalog, rules, select};
use game_content::Action;
use game_fighter::{Controller, Input, Rules, State};
use serde::{Deserialize, Serialize};

/// Durable snapshot of every mutable fact. Baked actions and rules are immutable
/// and are not part of a snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot(Controller);

/// Source-free Dog simulation over the shared controller and the authoritative
/// `game_fighter` reducer.
pub struct Simulation {
    actions: Vec<Action>,
    rules: Rules,
    controller: Controller,
}

impl Simulation {
    /// Construct from committed baked JSON and the generated Rules constructor.
    pub fn new() -> Result<Self, serde_json::Error> {
        let actions = catalog::load_baked()?;
        let rules = rules();
        let controller = Controller::new(State::new(&rules));
        Ok(Simulation { actions, rules, controller })
    }

    pub fn state(&self) -> &Controller {
        &self.controller
    }

    /// Animation frame for the current action, clamped to its frame count.
    pub fn frame(&self) -> usize {
        self.controller.frame(|index| frames(&self.actions, index))
    }

    /// One fixed tick through the shared controller.
    pub fn advance(&mut self, input: Input) -> &Controller {
        self.controller.advance(
            input,
            &self.rules,
            self.actions.len(),
            |index| frames(&self.actions, index),
            select,
        );
        &self.controller
    }

    pub fn save(&self) -> Snapshot {
        Snapshot(self.controller.clone())
    }

    pub fn load(&mut self, snapshot: &Snapshot) {
        self.controller = snapshot.0.clone();
    }
}

/// Frame count for one baked action index, or zero when the index is absent.
fn frames(actions: &[Action], index: usize) -> usize {
    actions.get(index).map_or(0, |action| action.frames.len())
}

/// Deterministic source-free tape. Each entry is one tick's buttons and axis.
/// It exercises idle, the three walk bands, dash reversal and dash dance, run,
/// brake, turn, the crouch lifecycle, jumpsquat, jump, fall, air jump and
/// landing.
pub fn tape() -> Vec<(u8, f32)> {
    let idle = (0u8, 0.0f32);
    let jump = (game_fighter::button::JUMP, 0.0f32);
    let down = (game_fighter::button::DOWN, 0.0f32);
    let mut ticks = Vec::new();
    ticks.extend(std::iter::repeat_n(idle, 6)); // Idle
    ticks.extend(std::iter::repeat_n((0, 0.3), 6)); // Walk slow band
    ticks.extend(std::iter::repeat_n((0, 0.5), 6)); // Walk middle band
    ticks.extend(std::iter::repeat_n((0, 0.7), 6)); // Walk fast band
    ticks.extend(std::iter::repeat_n(idle, 2));
    ticks.extend(std::iter::repeat_n((0, 0.9), 24)); // Dash, then Run
    ticks.extend([(0, -0.9)]); // Turn out of Run
    ticks.extend([(0, 0.9), (0, -0.9), (0, 0.9)]); // Dash dance / reversal
    ticks.extend(std::iter::repeat_n(idle, 30)); // Brake to Idle
    ticks.extend([down]); // CrouchEnter
    ticks.extend(std::iter::repeat_n(down, 5)); // CrouchHold
    ticks.extend(std::iter::repeat_n(idle, 3)); // CrouchExit to Idle
    ticks.extend([jump, jump, jump]); // Squat to takeoff
    ticks.extend(std::iter::repeat_n(idle, 6)); // Jump, settle to Fall
    ticks.extend([jump]); // Air jump
    ticks.extend(std::iter::repeat_n(idle, 60)); // Fall, land, recovery
    ticks
}

/// Run the deterministic tape on a fresh simulation and record the phase name
/// and selected action id for every tick.
pub fn observe_tape(simulation: &mut Simulation) -> Vec<(&'static str, usize)> {
    tape()
        .into_iter()
        .map(|(buttons, axis)| {
            let controller = simulation.advance(Input { buttons, axis });
            (controller.fighter.phase.name(), controller.action.id)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn input(tick: (u8, f32)) -> Input {
        Input { buttons: tick.0, axis: tick.1 }
    }

    /// Every phase and every role-selected action the tape claims to reach is
    /// observed; actions the runtime never selects are reported, not hidden.
    #[test]
    fn tape_reaches_every_locomotion_phase_and_action() {
        let mut simulation = Simulation::new().unwrap();
        let observed = observe_tape(&mut simulation);
        let phases: BTreeSet<&str> = observed.iter().map(|(phase, _)| *phase).collect();
        let actions: BTreeSet<usize> = observed.iter().map(|(_, action)| *action).collect();

        let expected_phases = [
            "Idle", "Walk", "Dash", "Run", "Brake", "Turn", "Squat", "CrouchEnter",
            "CrouchHold", "CrouchExit", "Landing", "Jump", "Fall", "AirJump",
        ];
        for phase in expected_phases {
            assert!(phases.contains(phase), "tape never reached {phase}: {phases:?}");
        }

        // The exact actions the selector proves live over the tape.
        let expected_actions = [0usize, 1, 2, 3, 4, 5, 6, 7, 8, 16, 17, 18, 19, 20, 21, 22];
        for action in expected_actions {
            assert!(actions.contains(&action), "tape never selected action {action}");
        }
        // AirAttack (12), LandingLight (23) and LandingRecovery (24) are
        // bound-only: this runtime does not select them.
        for action in [12usize, 23, 24] {
            assert!(!actions.contains(&action), "action {action} was selected but is claimed bound-only");
        }
    }

    /// Suffix replay from a snapshot is exact, and a serde roundtrip restores
    /// every mutable fact.
    #[test]
    fn snapshot_suffix_replay_and_json_roundtrip_are_exact() {
        let ticks = tape();
        let mut simulation = Simulation::new().unwrap();
        let mut states = Vec::new();
        let mut saves = Vec::new();
        for (tick, entry) in ticks.iter().enumerate() {
            if tick % 13 == 0 {
                saves.push((tick, simulation.save()));
            }
            states.push(simulation.advance(input(*entry)).clone());
        }
        for (tick, snapshot) in saves {
            simulation.load(&snapshot);
            for (offset, entry) in ticks[tick..].iter().enumerate() {
                assert_eq!(simulation.advance(input(*entry)), &states[tick + offset]);
            }
        }
        for state in states.iter().step_by(7) {
            let json = serde_json::to_vec(state).unwrap();
            let restored: game_fighter::Controller =
                serde_json::from_slice(&json).unwrap();
            assert_eq!(&restored, state);
        }
        for state in states.iter().step_by(7) {
            let json = serde_json::to_vec(&Snapshot(state.clone())).unwrap();
            let restored: Snapshot = serde_json::from_slice(&json).unwrap();
            assert_eq!(restored.0, *state);
        }
    }
}

