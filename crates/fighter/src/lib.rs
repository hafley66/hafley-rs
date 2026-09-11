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

#[path = "2_advance.rs"]
mod advance_impl;
#[path = "0_rules.rs"]
pub mod rules;
#[path = "1c_air.rs"]
pub mod air;
#[path = "1d_combat.rs"]
pub mod combat;
#[path = "1a_chart.rs"]
pub mod chart;
#[path = "1b_ground.rs"]
pub mod ground;
#[path = "4_ground_chart.rs"]
pub mod ground_chart;
#[path = "5_status.rs"]
pub mod status;
#[path = "6_qualification.rs"]
pub mod qualification;
#[path = "3_slice.rs"]
pub mod slice;
#[path = "1_state.rs"]
pub mod state;

pub use chart::{Action, CrouchChart, CrouchSlice, Facts};
pub use combat::{CombatState, Hit, apply_hit};
pub use rules::Rules;
pub use slice::{FighterEvent, FighterSlice, MovementEffect, MovementSlice};
pub use state::{Input, Phase, State, button};

pub fn advance(state: &mut State, input: Input, rules: &Rules) {
    use redux::Slice;
    MovementSlice::reduce(state, input, rules, &mut |_| unreachable!());
}
