//! Generic rollback coordination around an application-supplied deterministic reducer.

use ggrs::{Config, GgrsRequest, PlayerType, PredictRepeatLast, SessionBuilder, SyncTestSession};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fmt::Debug;
use std::hash::Hash;
use std::marker::PhantomData;

/// Shared request execution. Host observers run outside this operation so corrected
/// presentation/history can be recorded without storing IO in simulation snapshots.
pub fn apply_request<C: Config>(
    state: &mut C::State,
    request: GgrsRequest<C>,
    checksum: impl FnOnce(&C::State) -> u128,
    advance: impl FnOnce(&mut C::State, Vec<(C::Input, ggrs::InputStatus)>),
) where C::State: Clone {
    match request {
        GgrsRequest::SaveGameState { cell, frame } => {
            cell.save(frame, Some(state.clone()), Some(checksum(state)));
        }
        GgrsRequest::LoadGameState { cell, .. } => {
            *state = cell.load().expect("ggrs load on a saved frame");
        }
        GgrsRequest::AdvanceFrame { inputs } => advance(state, inputs),
    }
}

pub trait RollbackSim: 'static {
    type State: Clone + Send + Sync;
    type Input: Copy + Clone + PartialEq + Default + Serialize + DeserializeOwned + Send + Sync;
    type Config: Clone;

    fn initial(config: &Self::Config) -> Self::State;
    fn advance(state: &Self::State, inputs: &[Self::Input], config: &Self::Config) -> Self::State;
    fn checksum(state: &Self::State) -> u128;
}

/// ggrs configuration parameterized independently by simulation and transport address.
pub struct GgrsConfig<S: RollbackSim, A = usize>(PhantomData<(S, A)>);

impl<S, A> Config for GgrsConfig<S, A>
where
    S: RollbackSim,
    A: Clone + PartialEq + Eq + Hash + Debug + Send + Sync + 'static,
{
    type Input = S::Input;
    type InputPredictor = PredictRepeatLast;
    type State = S::State;
    type Address = A;
}

impl<S: RollbackSim, A> Debug for GgrsConfig<S, A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GgrsConfig")
    }
}

pub struct Game<S: RollbackSim> {
    pub state: S::State,
    pub cfg: S::Config,
}

impl<S: RollbackSim> Game<S> {
    pub fn new(cfg: S::Config) -> Self {
        Self {
            state: S::initial(&cfg),
            cfg,
        }
    }

    pub fn from_state(state: S::State, cfg: S::Config) -> Self {
        Self { state, cfg }
    }

    pub fn handle<A>(&mut self, requests: Vec<GgrsRequest<GgrsConfig<S, A>>>)
    where
        A: Clone + PartialEq + Eq + Hash + Debug + Send + Sync + 'static,
    {
        self.handle_observed(requests, |_, _| {});
    }

    /// Observe saved frame checksums, including corrected frames during rollback catch-up.
    pub fn handle_observed<A>(&mut self, requests: Vec<GgrsRequest<GgrsConfig<S, A>>>, mut saved: impl FnMut(ggrs::Frame, u128))
    where
        A: Clone + PartialEq + Eq + Hash + Debug + Send + Sync + 'static,
    {
        for request in requests {
            let saved_frame = match &request {
                GgrsRequest::SaveGameState { frame, .. } => Some(*frame),
                _ => None,
            };
            apply_request(&mut self.state, request, S::checksum, |state, inputs| {
                    let wires: Vec<S::Input> = inputs.iter().map(|(input, _)| *input).collect();
                    *state = S::advance(state, &wires, &self.cfg);
            });
            if let Some(frame) = saved_frame {
                saved(frame, S::checksum(&self.state));
            }
        }
    }
}

pub fn synctest_session<S: RollbackSim>(
    num_players: usize,
    check_distance: usize,
) -> SyncTestSession<GgrsConfig<S>> {
    let mut builder = SessionBuilder::<GgrsConfig<S>>::new()
        .with_num_players(num_players)
        .expect("valid player count")
        .with_check_distance(check_distance);
    for handle in 0..num_players {
        builder = builder
            .add_player(PlayerType::Local, handle)
            .expect("add local player");
    }
    builder.start_synctest_session().expect("synctest session")
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Counter;
    impl RollbackSim for Counter {
        type State = i64;
        type Input = i8;
        type Config = ();
        fn initial(_: &()) -> i64 { 0 }
        fn advance(state: &i64, inputs: &[i8], _: &()) -> i64 {
            state + inputs.iter().map(|&n| i64::from(n)).sum::<i64>()
        }
        fn checksum(state: &i64) -> u128 { *state as u128 }
    }

    #[test]
    fn shared_executor_restores_and_replays_changing_inputs() {
        let mut session = synctest_session::<Counter>(2, 7);
        let mut game = Game::<Counter>::new(());
        let mut expected = 0;
        let mut saves = std::collections::BTreeMap::new();
        let mut replayed_saves = 0;
        for tick in 0..100 {
            let inputs = [(tick % 3) as i8, -((tick % 2) as i8)];
            for (player, input) in inputs.into_iter().enumerate() {
                session.add_local_input(player, input).unwrap();
            }
            game.handle_observed(session.advance_frame().unwrap(), |frame, checksum| {
                if let Some(previous) = saves.insert(frame, checksum) {
                    assert_eq!(checksum, previous);
                    replayed_saves += 1;
                }
            });
            expected += inputs.into_iter().map(i64::from).sum::<i64>();
            assert_eq!(game.state, expected, "tick {tick}");
        }
        assert!(replayed_saves > 0, "must execute replay, not just forward steps");
    }
}
