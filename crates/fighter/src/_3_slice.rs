//! Redux entry points: dispatch `Input` through `MovementSlice`, or either an
//! `Input` or a typed `Hit` through `FighterSlice`. The state method
//! `State::advance` is the same scan; `_1d_combat::apply_hit` is the hit scan.

use redux::{Never, Slice};
use serde::{Deserialize, Serialize};

use crate::_0_rules::Rules;
use crate::_1_state::{Input, State};
use crate::_1d_combat::{Hit, apply_hit};

pub struct MovementSlice;

#[derive(Serialize, Deserialize)]
pub enum MovementEffect {}

impl Slice for MovementSlice {
    type Context<'a> = &'a Rules;
    type State = crate::_1_state::State;
    type Event = Input;
    type Output = ();
    type Effect = MovementEffect;

    fn reduce(
        st: &mut Self::State,
        ev: Self::Event,
        cx: Self::Context<'_>,
        _fx: &mut impl FnMut(Self::Effect),
    ) -> Self::Output {
        st.advance(ev, cx)
    }
}

/// One dispatcher event: ordinary movement input or a resolved hit.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum FighterEvent {
    Input(Input),
    Hit(Hit),
}

/// Combined dispatcher. The slice owns the mutation: movement input advances
/// locomotion, a hit resolves and applies its outcome with no app-side writes.
pub struct FighterSlice;

impl Slice for FighterSlice {
    type Context<'a> = &'a Rules;
    type State = State;
    type Event = FighterEvent;
    type Output = ();
    type Effect = Never;

    fn reduce(
        st: &mut Self::State,
        ev: Self::Event,
        cx: Self::Context<'_>,
        _fx: &mut impl FnMut(Self::Effect),
    ) -> Self::Output {
        match ev {
            FighterEvent::Input(input) => st.advance(input, cx),
            FighterEvent::Hit(hit) => {
                apply_hit(st, &hit);
            }
        }
    }
}
