//! Offline qualification over the existing RollbackSim API. No game or codec ownership.
use crate::{Game, RollbackSim, synctest_session};
use std::fmt::Debug;

pub struct Trace<State> {
    pub states: Vec<State>,
    pub restore_requests: Vec<Vec<i32>>,
    pub advances: Vec<usize>,
}

/// Tape rows are ticks; columns are stable local player handles. Retains one
/// complete state per forward tick. Fixture errors panic; GGRS errors propagate.
pub fn run<S: RollbackSim>(
    config: &S::Config,
    tape: &[Vec<S::Input>],
    distance: usize,
) -> Result<Trace<S::State>, ggrs::GgrsError>
where
    S::State: Debug + PartialEq,
{
    let players = tape.first().expect("nonempty proof tape").len();
    assert!(players > 0 && tape.iter().all(|row| row.len() == players));
    let mut direct = S::initial(config);
    let mut game = Game::<S>::new(config.clone());
    let mut session = synctest_session::<S>(players, distance);
    let mut trace = Trace {
        states: Vec::new(),
        restore_requests: Vec::new(),
        advances: Vec::new(),
    };
    for (tick, inputs) in tape.iter().enumerate() {
        for (player, input) in inputs.iter().copied().enumerate() {
            session.add_local_input(player, input)?;
        }
        let requests = session.advance_frame()?;
        trace.restore_requests.push(
            requests
                .iter()
                .filter_map(|r| match r {
                    ggrs::GgrsRequest::LoadGameState { frame, .. } => Some(*frame),
                    _ => None,
                })
                .collect(),
        );
        trace.advances.push(
            requests
                .iter()
                .filter(|r| matches!(r, ggrs::GgrsRequest::AdvanceFrame { .. }))
                .count(),
        );
        game.handle(requests);
        direct = S::advance(&direct, inputs, config);
        assert_eq!(game.state, direct, "GGRS corrected state at {tick}");
        trace.states.push(direct.clone());
    }
    assert!(
        trace.restore_requests.iter().any(|r| !r.is_empty()),
        "tape must exercise restore"
    );
    Ok(trace)
}

/// Checkpoints refer to post-tick states. Each restored instance lives for its
/// remaining suffix only. The caller supplies Clone or an actual codec roundtrip.
pub fn restore_suffixes<S: RollbackSim, E>(
    config: &S::Config,
    tape: &[Vec<S::Input>],
    states: &[S::State],
    checkpoints: &[usize],
    mut restore: impl FnMut(&S::State) -> Result<S::State, E>,
) -> Result<usize, E>
where
    S::State: Debug + PartialEq,
{
    assert_eq!(tape.len(), states.len());
    let mut replayed = 0;
    for &checkpoint in checkpoints {
        let expected = states.get(checkpoint).expect("checkpoint inside trace");
        let mut state = restore(expected)?;
        assert_eq!(&state, expected, "restored checkpoint {checkpoint}");
        for tick in checkpoint + 1..states.len() {
            state = S::advance(&state, &tape[tick], config);
            assert_eq!(state, states[tick], "restored suffix at tick {tick}");
            replayed += 1;
        }
    }
    Ok(replayed)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Counter;
    impl RollbackSim for Counter {
        type State = i64;
        type Input = i8;
        type Config = ();
        fn initial(_: &()) -> i64 {
            0
        }
        fn advance(state: &i64, inputs: &[i8], _: &()) -> i64 {
            state + inputs.iter().map(|&v| i64::from(v)).sum::<i64>()
        }
        fn checksum(state: &i64) -> u128 {
            *state as u128
        }
    }
    #[test]
    fn multiplayer_replay_and_restore_failures() {
        let tape: Vec<_> = (0..32).map(|_| vec![2, -1]).collect();
        let trace = run::<Counter>(&(), &tape, 7).unwrap();
        assert_eq!(trace.states, (1..=32).collect::<Vec<_>>());
        assert!(trace.advances.iter().sum::<usize>() > tape.len());
        let replayed =
            restore_suffixes::<Counter, ()>(&(), &tape, &trace.states, &[0, 10, 31], |s| Ok(*s))
                .unwrap();
        assert_eq!(replayed, 52);
        assert_eq!(
            restore_suffixes::<Counter, _>(&(), &tape, &trace.states, &[0], |_| Err("decode")),
            Err("decode")
        );
        assert!(
            std::panic::catch_unwind(|| {
                restore_suffixes::<Counter, ()>(&(), &tape, &trace.states, &[0], |s| Ok(s + 1))
                    .unwrap();
            })
            .is_err()
        );
    }
}
