//! Generic fighter locomotion: caller-supplied `Rules`, tick-advancing `State`.
//!
//! Phase rules follow the Melee decomp common fighter code (ftCommon) in shape
//! but do not claim exact Melee equivalence; every numeric value comes from
//! `Rules`. Ground plane is y=0, x grows right, y grows up; units and axes are
//! caller-owned. Fixed tick: `State::advance` is one simulation step.
//!
//! Traction semantics (verified against the decomp, correcting the earlier
//! report): general ground friction is a LINEAR deceleration of constant
//! magnitude toward zero (`ftCommon_ApplyFrictionGround`,
//! ft/ftcommon.c:50-60). The MULTIPLICATIVE decay (`gr_vel -= gr_vel * mul *
//! friction`, ft/kinds/ftCommon/ftCo_Dash.c:142-144) is dash-sustain only.

pub mod _0_rules;
pub mod _1_state;
pub mod _1b_ground;
pub mod _1c_air;
pub mod _1d_combat;
mod _2_advance;
pub mod _3_slice;
pub mod _4_ground_chart;
pub mod _5_status;
pub mod _6_qualification;

pub use _0_rules::Rules;
pub use _1_state::{ActionState, Input, Phase, State, button};
pub use _1d_combat::{CombatState, Hit, apply_hit};
pub use _3_slice::{
    ActionContext, ActionSlice, CombatSlice, FighterEvent, FighterSlice, MovementEffect,
    MovementSlice, frame, tick,
};

impl State {
    /// Compatibility seam for callers that dispatch one movement tick.
    pub fn advance(&mut self, input: Input, rules: &Rules) {
        use redux::Slice;
        MovementSlice::reduce(self, input, rules, &mut |_| unreachable!());
    }
}

pub fn advance(state: &mut State, input: Input, rules: &Rules) {
    use redux::Slice;
    MovementSlice::reduce(state, input, rules, &mut |_| unreachable!());
}
