//! Deterministic runtime tests for the source-free Dog slice.

use super::*;
use game_fighter::{Input, Phase};
use std::collections::BTreeSet;

fn input(pair: (u8, f32)) -> Input {
    Input { buttons: pair.0, axis: pair.1 }
}

fn drive(sim: &mut Simulation, tape: &[(u8, f32)]) -> Vec<ControllerState> {
    tape.iter()
        .map(|&(buttons, axis)| sim.advance(Input { buttons, axis }).clone())
        .collect()
}

/// The committed bake exposes every action role the runtime can select; no
/// action has an empty frame list to clamp around.
#[test]
fn runtime_loads_committed_content_with_usable_frames() {
    let sim = Simulation::new().expect("committed Dog baked actions");
    assert_eq!(sim.actions().len(), catalog::ACTION_COUNT);
    assert!(sim.actions().iter().all(|action| !action.frames.is_empty()));
    assert_eq!(sim.actions()[0].frames.len(), 241);
}

/// The tape must actually reach all 14 phases and all 16 selected actions.
/// Anything not reached is reported by name in the failure message rather than
/// covered with a substitute expectation.
#[test]
fn tape_selects_every_reachable_phase_and_replays_exactly() {
    let tape = tape();
    let mut sim = Simulation::new().expect("committed Dog baked actions");
    let states = drive(&mut sim, &tape);

    let observed_phases: BTreeSet<&'static str> =
        states.iter().map(|state| state.fighter.phase.name()).collect();
    let expected_phases: BTreeSet<&'static str> =
        Phase::ALL.iter().map(|phase| phase.name()).collect();
    let unreachable: Vec<_> = expected_phases.difference(&observed_phases).collect();
    assert_eq!(observed_phases, expected_phases, "unreachable phases: {unreachable:?}");

    let observed_actions: BTreeSet<usize> = states.iter().map(|state| state.action).collect();
    let expected_actions: BTreeSet<usize> =
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 16, 17, 18, 19, 20, 21, 22].into_iter().collect();
    let unselected: Vec<_> = expected_actions.difference(&observed_actions).collect();
    assert_eq!(observed_actions, expected_actions, "unselected actions: {unselected:?}");

    // AirAttack (12), LandingLight (23) and LandingRecovery (24) are bound-only
    // in this runtime: the plain Phase/axis selector never returns them.
    for bound_only in [12usize, 23, 24] {
        assert!(
            !observed_actions.contains(&bound_only),
            "bound-only action {bound_only} was selected by the runtime",
        );
    }

    // Exact suffix replay from a live rollback snapshot.
    let cut = tape.len() / 2;
    let mut replay = Simulation::new().expect("committed Dog baked actions");
    let _ = drive(&mut replay, &tape[..cut]);
    let snapshot = replay.save();
    for (offset, state) in states[cut..].iter().enumerate() {
        assert_eq!(
            replay.advance(input(tape[cut + offset])),
            state,
            "live suffix diverged at tape tick {}",
            cut + offset,
        );
    }
    replay.load(&snapshot);
    for (offset, state) in states[cut..].iter().enumerate() {
        assert_eq!(
            replay.advance(input(tape[cut + offset])),
            state,
            "snapshot suffix diverged at tape tick {}",
            cut + offset,
        );
    }

    // Snapshot and every sampled state survive a JSON roundtrip exactly.
    let encoded = serde_json::to_string(&snapshot).expect("serialize Dog snapshot");
    let decoded: Snapshot = serde_json::from_str(&encoded).expect("deserialize Dog snapshot");
    assert_eq!(decoded, snapshot);
    replay.load(&decoded);
    for (offset, state) in states[cut..].iter().enumerate() {
        assert_eq!(replay.advance(input(tape[cut + offset])), state);
    }
    for state in states.iter().step_by(17) {
        let restored: ControllerState =
            serde_json::from_str(&serde_json::to_string(state).unwrap()).unwrap();
        assert_eq!(&restored, state);
    }
}

/// A binding change or Phase change restarts the pose clock at frame 0.
#[test]
fn animation_resets_on_every_action_or_phase_transition() {
    let mut sim = Simulation::new().expect("committed Dog baked actions");
    let states = drive(&mut sim, &tape());
    for pair in states.windows(2) {
        let (previous, current) = (&pair[0], &pair[1]);
        if current.action != previous.action || current.fighter.phase != previous.fighter.phase {
            assert_eq!(
                current.animation, 0,
                "animation did not reset into {:?}/{}",
                current.fighter.phase, current.action,
            );
        }
    }
}
