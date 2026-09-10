//! Rollback state and input vocabulary. Every mutable cause of future
//! gameplay for this slice lives in `State`; `Rules` is immutable shared
//! content owned by the caller.

use serde::{Deserialize, Serialize};

use crate::rules::Rules;

/// Input bits: bit 1 jump, bit 2 attack, bit 4 down.
pub mod button {
    pub const JUMP: u8 = 1 << 0;
    pub const ATTACK: u8 = 1 << 1;
    pub const DOWN: u8 = 1 << 2;
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Input {
    pub buttons: u8,
    /// Horizontal stick, -1..=1.
    pub axis: f32,
}

impl Input {
    pub fn from_player(p: &game_input::PlayerInput) -> Self {
        Input {
            buttons: p.buttons as u8,
            axis: game_input::dequantize_axis(p.axes[0]),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Idle,
    Walk,
    Dash,
    Run,
    Brake,
    Turn,
    Squat,
    Crouch,
    Landing,
    Jump,
    Fall,
    AirJump,
}

impl Phase {
    pub fn grounded(self) -> bool {
        matches!(
            self,
            Phase::Idle
                | Phase::Walk
                | Phase::Dash
                | Phase::Run
                | Phase::Brake
                | Phase::Turn
                | Phase::Squat
                | Phase::Crouch
                | Phase::Landing
        )
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub phase: Phase,
    pub phase_tick: u32,
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub facing: f32,
    pub jumps_left: u8,
    pub short_hop: bool,
    pub input_history: game_input::History,
    pub fast_fall: bool,
}

impl Default for State {
    fn default() -> Self {
        State {
            phase: Phase::Idle,
            phase_tick: 0,
            position: [0.0, 0.0],
            velocity: [0.0, 0.0],
            facing: 1.0,
            jumps_left: 1,
            short_hop: false,
            input_history: game_input::History::default(),
            fast_fall: false,
        }
    }
}

impl State {
    pub fn new(rules: &Rules) -> Self {
        let mut state = Self::default();
        state.jumps_left = rules.max_jumps;
        state
    }

    pub fn grounded(&self) -> bool {
        self.phase.grounded()
    }

    pub fn enter(&mut self, phase: Phase) {
        tracing::debug!(from = ?self.phase, to = ?phase, "phase transition");
        self.phase = phase;
        self.phase_tick = 0;
    }
}
