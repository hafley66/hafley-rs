use ggrs::{Config, Frame, GameStateCell, GgrsRequest, InputStatus, PredictRepeatLast, SessionBuilder};
use serde::{Deserialize, Serialize};
use std::{collections::hash_map::DefaultHasher, hash::{Hash, Hasher}, net::SocketAddr};

#[repr(C)]
#[derive(Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabInput { pub bits: u8 }

#[derive(Copy, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LabState { pub frame: i32, pub positions: [i32; 2], pub accumulator: u64 }

pub struct LabConfig;
impl Config for LabConfig {
    type Input = LabInput;
    type InputPredictor = PredictRepeatLast;
    type State = LabState;
    type Address = SocketAddr;
}

fn checksum(state: &LabState) -> u128 {
    let mut hasher = DefaultHasher::new();
    state.hash(&mut hasher);
    hasher.finish() as u128
}

pub fn input_for(frame: i32, player: usize) -> LabInput {
    LabInput { bits: (((frame as usize * 17 + player * 31) ^ (frame as usize >> 2)) & 3) as u8 }
}

pub struct Harness { pub state: LabState, pub loads: usize, corrupt_next_load: bool }

impl Harness {
    pub fn new(corrupt_next_load: bool) -> Self {
        Self { state: LabState::default(), loads: 0, corrupt_next_load }
    }

    pub fn handle(&mut self, requests: Vec<GgrsRequest<LabConfig>>) {
        for request in requests {
            match request {
                GgrsRequest::SaveGameState { cell, frame } => self.save(cell, frame),
                GgrsRequest::LoadGameState { cell, .. } => {
                    self.state = cell.load().unwrap();
                    self.loads += 1;
                    if self.corrupt_next_load {
                        self.state.positions[0] ^= 1;
                        self.corrupt_next_load = false;
                    }
                }
                GgrsRequest::AdvanceFrame { inputs } => self.advance(inputs),
            }
        }
    }

    fn save(&self, cell: GameStateCell<LabState>, frame: Frame) {
        assert_eq!(self.state.frame, frame);
        cell.save(frame, Some(self.state), Some(checksum(&self.state)));
    }

    fn advance(&mut self, inputs: Vec<(LabInput, InputStatus)>) {
        for (player, (input, _)) in inputs.into_iter().enumerate() {
            let direction = match input.bits & 3 { 0 => -2, 1 => -1, 2 => 1, _ => 2 };
            self.state.positions[player] += direction;
            self.state.accumulator = self.state.accumulator.rotate_left(5)
                ^ ((input.bits as u64) << (player * 8)) ^ self.state.frame as u64;
        }
        self.state.frame += 1;
    }
}

pub fn session() -> ggrs::SyncTestSession<LabConfig> {
    SessionBuilder::<LabConfig>::new()
        .with_num_players(2)
        .unwrap()
        .with_max_prediction_window(12)
        .with_check_distance(8)
        .start_synctest_session().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_test_rolls_back_deterministic_state_without_mismatch() {
        let mut session = session();
        let mut game = Harness::new(false);
        for frame in 0..180 {
            session.add_local_input(0, input_for(frame, 0)).unwrap();
            session.add_local_input(1, input_for(frame, 1)).unwrap();
            game.handle(session.advance_frame().unwrap());
        }
        assert_eq!(game.state.frame, 180);
        assert!(game.loads > 100);
    }

    #[test]
    fn sync_test_detects_deliberately_corrupted_loaded_state() {
        let mut session = session();
        let mut game = Harness::new(true);
        let mut mismatch = None;
        for frame in 0..40 {
            session.add_local_input(0, input_for(frame, 0)).unwrap();
            session.add_local_input(1, input_for(frame, 1)).unwrap();
            match session.advance_frame() {
                Ok(requests) => game.handle(requests),
                Err(error) => { mismatch = Some(error); break; }
            }
        }
        assert!(matches!(mismatch, Some(ggrs::GgrsError::MismatchedChecksum { .. })));
    }
}
