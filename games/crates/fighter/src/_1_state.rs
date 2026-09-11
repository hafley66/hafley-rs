//! Rollback state and input vocabulary. Every mutable cause of future
//! gameplay for this slice lives in `State`; `Rules` is immutable shared
//! content owned by the caller.

use serde::{Deserialize, Serialize};

use crate::_0_rules::Rules;
use crate::_1d_combat::CombatState;

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
    /// `ftCo_MS_JumpSquat`/`ftCo_MS_KneeBend` takeoff preparation.
    Squat,
    /// `ftCo_MS_Squat`, the crouch-down animation.
    CrouchEnter,
    /// `ftCo_MS_SquatWait`, the held crouch.
    CrouchHold,
    /// `ftCo_MS_SquatRv`, the stand-from-crouch animation.
    CrouchExit,
    Landing,
    Jump,
    Fall,
    AirJump,
}

impl Phase {
    /// Declaration order of the serialized `Phase` variants. This is the
    /// executable inventory; callers must not restate it.
    pub const ALL: [Phase; 14] = [
        Phase::Idle,
        Phase::Walk,
        Phase::Dash,
        Phase::Run,
        Phase::Brake,
        Phase::Turn,
        Phase::Squat,
        Phase::CrouchEnter,
        Phase::CrouchHold,
        Phase::CrouchExit,
        Phase::Landing,
        Phase::Jump,
        Phase::Fall,
        Phase::AirJump,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Phase::Idle => "Idle",
            Phase::Walk => "Walk",
            Phase::Dash => "Dash",
            Phase::Run => "Run",
            Phase::Brake => "Brake",
            Phase::Turn => "Turn",
            Phase::Squat => "Squat",
            Phase::CrouchEnter => "CrouchEnter",
            Phase::CrouchHold => "CrouchHold",
            Phase::CrouchExit => "CrouchExit",
            Phase::Landing => "Landing",
            Phase::Jump => "Jump",
            Phase::Fall => "Fall",
            Phase::AirJump => "AirJump",
        }
    }

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
                | Phase::CrouchEnter
                | Phase::CrouchHold
                | Phase::CrouchExit
                | Phase::Landing
        )
    }
}

/// Selected action id and its animation frame, kept together so the pair is
/// cloned, snapshotted and serialized as one crate-owned fact.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionState {
    pub id: usize,
    pub frame: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub action: ActionState,
    pub phase: Phase,
    pub phase_tick: u32,
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub facing: f32,
    pub jumps_left: u8,
    pub short_hop: bool,
    pub input_history: game_input::History,
    pub fast_fall: bool,
    #[serde(default)]
    pub combat: CombatState,
}

impl Default for State {
    fn default() -> Self {
        State {
            action: ActionState::default(),
            phase: Phase::Idle,
            phase_tick: 0,
            position: [0.0, 0.0],
            velocity: [0.0, 0.0],
            facing: 1.0,
            jumps_left: 1,
            short_hop: false,
            input_history: game_input::History::default(),
            fast_fall: false,
            combat: CombatState::default(),
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
