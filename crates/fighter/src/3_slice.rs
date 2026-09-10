//! Redux entry point: dispatch `Input` through `MovementSlice` with the
//! `Rules` as context; the state method `State::advance` is the same scan.

use redux::Slice;
use serde::{Deserialize, Serialize};

use crate::rules::Rules;
use crate::state::Input;

pub struct MovementSlice;

#[derive(Serialize, Deserialize)]
pub enum MovementEffect {}

impl Slice for MovementSlice {
    type Context<'a> = &'a Rules;
    type State = crate::state::State;
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
