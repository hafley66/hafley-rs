//! Generic rollback coordination around an application-supplied deterministic reducer.

use ggrs::{Config, GgrsRequest, PlayerType, PredictRepeatLast, SessionBuilder, SyncTestSession};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fmt::Debug;
use std::hash::Hash;
use std::marker::PhantomData;

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
        for request in requests {
            match request {
                GgrsRequest::SaveGameState { cell, frame } => {
                    cell.save(
                        frame,
                        Some(self.state.clone()),
                        Some(S::checksum(&self.state)),
                    );
                }
                GgrsRequest::LoadGameState { cell, .. } => {
                    self.state = cell.load().expect("ggrs load on a saved frame");
                }
                GgrsRequest::AdvanceFrame { inputs } => {
                    let wires: Vec<S::Input> = inputs.iter().map(|(input, _)| *input).collect();
                    self.state = S::advance(&self.state, &wires, &self.cfg);
                }
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
