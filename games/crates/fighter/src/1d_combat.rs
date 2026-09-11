//! Shared fighter combat response: damage percent, hitlag, hitstun, tumble and
//! the launch response, plus the typed hit event the reducer dispatches.
//!
//! [`apply_hit`] is the one seam: it resolves through `game_combat::resolve_hit`
//! using this state's percent and grounded fact, then applies the outcome to the
//! same state. Launch velocity is written to the canonical `State::velocity`;
//! [`CombatState`] keeps the remaining mutable combat causes and duplicates
//! percent, hitlag or hitstun nowhere.
//!
//! Unsupported in this cut, not to be invented here: shielding/guard, ASDI/SDI
//! (only two-axis trajectory DI), knockback decay, landing tech, hitstun
//! action-state fidelity, hitbox/collision detection and contact suppression.

use game_combat::{DefenseInput, HitOutcome, ResolvePolicy, Strike, Target};
use serde::{Deserialize, Serialize};

use crate::state::State;

/// Rollback-owned combat response.
#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CombatState {
    /// Accumulated damage percent, the knockback formula's `p`.
    pub percent: f32,
    /// Remaining impact-freeze frames.
    pub hitlag: u32,
    /// Remaining hitstun frames.
    pub hitstun: u32,
    /// Resolved launch response magnitude in knockback units, retained for
    /// thresholds and presentation.
    pub knockback: f32,
    /// Whether the current launch tumbles.
    pub tumble: bool,
}

/// One typed hit input: the strike, the caller-supplied target weight and
/// ruleset policy, and the victim's quantized input used for trajectory DI.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    pub strike: Strike,
    pub weight: f32,
    pub policy: ResolvePolicy,
    pub input: game_input::PlayerInput,
}

impl Hit {
    /// The victim's defensive stick, dequantized to `[x, y]`, y up.
    pub fn defense_input(&self) -> DefenseInput {
        DefenseInput {
            stick: [
                game_input::dequantize_axis(self.input.axes[0]),
                game_input::dequantize_axis(self.input.axes[1]),
            ],
        }
    }
}

/// Resolve `hit` against `state`'s percent and grounded fact, then apply the
/// outcome to `state`. Pure and reducer-compatible: no allocation, no I/O.
#[tracing::instrument(target = "game_fighter::combat", level = "trace", skip_all)]
pub fn apply_hit(state: &mut State, hit: &Hit) -> HitOutcome {
    let outcome = game_combat::resolve_hit(
        hit.strike,
        Target {
            percent: state.combat.percent,
            weight: hit.weight,
            grounded: state.grounded(),
        },
        hit.defense_input(),
        hit.policy,
    );
    state.combat.percent = outcome.percent_after;
    state.combat.hitlag = outcome.hitlag;
    state.combat.hitstun = outcome.hitstun;
    state.combat.knockback = outcome.knockback;
    state.combat.tumble = outcome.tumble;
    state.velocity = outcome.velocity;
    outcome
}
