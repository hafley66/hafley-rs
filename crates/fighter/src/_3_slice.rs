//! Redux entry points: dispatch `Input` through `MovementSlice`, either an
//! `Input` or a typed `Hit` through `FighterSlice`, or a full fighter tick
//! through [`ActionSlice`]. The state method `State::advance` is the same scan;
//! `_1d_combat::apply_hit` is the hit scan.

use redux::{Never, Slice};
use serde::{Deserialize, Serialize};

use crate::_0_rules::Rules;
use crate::_1_state::{Input, Phase, State};
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

/// Immutable caller context for one action tick: the selectable-action count,
/// the frame-count lookup and the phase/axis selector. These stay outside the
/// rollback state and are never snapshotted.
#[derive(Clone, Copy)]
pub struct ActionContext<'a> {
    pub rules: &'a Rules,
    pub action_count: usize,
    pub frame_count: &'a dyn Fn(usize) -> usize,
    pub select: &'a dyn Fn(Phase, f32) -> Option<usize>,
}

/// One fixed fighter tick as a redux slice over the canonical [`State`]. In
/// order: the empty-catalog gate, capture the previous phase, the movement and
/// combat gate through [`MovementSlice`], action selection for the resulting
/// phase and axis, then the animation frame reset or advance. The selector and
/// frame lookup are caller context, so no stateful wrapper holds them.
pub struct ActionSlice;

impl Slice for ActionSlice {
    type Context<'a> = ActionContext<'a>;
    type State = State;
    type Event = Input;
    type Output = ();
    type Effect = Never;

    fn reduce(
        st: &mut Self::State,
        ev: Self::Event,
        cx: Self::Context<'_>,
        _fx: &mut impl FnMut(Self::Effect),
    ) -> Self::Output {
        if cx.action_count == 0 {
            return;
        }
        let previous = st.phase;
        MovementSlice::reduce(st, ev, cx.rules, &mut |never| match never {});
        let phase = st.phase;
        let selected = (cx.select)(phase, ev.axis)
            .filter(|action| *action < cx.action_count)
            .unwrap_or(st.action.id.min(cx.action_count - 1));
        let frames = (cx.frame_count)(selected);
        if selected != st.action.id || phase != previous || frames == 0 {
            st.action.frame = 0;
        } else {
            st.action.frame = (st.action.frame + 1) % frames;
        }
        st.action.id = selected;
    }
}

/// The one top-level fighter tick entry: movement/combat gate, then action
/// selection and frame reset/advance through [`ActionSlice`].
pub fn tick(
    state: &mut State,
    input: Input,
    rules: &Rules,
    action_count: usize,
    frame_count: &dyn Fn(usize) -> usize,
    select: &dyn Fn(Phase, f32) -> Option<usize>,
) {
    let cx = ActionContext {
        rules,
        action_count,
        frame_count,
        select,
    };
    <ActionSlice as Slice>::reduce(state, input, cx, &mut |never| match never {});
}

/// Animation frame for the selected action, clamped to its frame count.
pub fn frame(state: &State, frame_count: impl Fn(usize) -> usize) -> usize {
    let frames = frame_count(state.action.id);
    if frames == 0 {
        0
    } else {
        state.action.frame.min(frames - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ActionState;

    fn rules() -> Rules {
        Rules {
            walk_init_vel: 0.4,
            walk_accel: 0.1,
            walk_max_vel: 0.8,
            walk_stick_threshold: 0.3,
            dash_stick_threshold: 0.8,
            dash_initial_velocity: 2.0,
            dash_accel_base: 0.05,
            dash_accel_mul: 0.03,
            dash_max_velocity: 1.6,
            dash_ticks: 4,
            ground_friction: 0.1,
            dash_friction_mul: 0.5,
            ground_max_horizontal_velocity: 4.0,
            turn_ticks: 3,
            jump_startup_time: 4,
            crouch_enter_ticks: 3,
            crouch_exit_ticks: 2,
            jump_h_initial_velocity: 0.5,
            jump_h_max_velocity: 1.2,
            jump_v_initial_velocity: 3.0,
            hop_v_initial_velocity: 2.0,
            ground_to_air_jump_momentum_multiplier: 0.5,
            max_jumps: 2,
            air_jump_v_multiplier: 1.0,
            air_jump_h_multiplier: 0.9,
            gravity: 0.2,
            terminal_velocity: 2.0,
            fast_fall_velocity: 2.8,
            air_drift_stick_mul: 0.06,
            air_drift_base: 0.02,
            air_drift_max: 1.0,
            aerial_friction: 0.02,
            landing_lag: 4,
        }
    }

    fn input(axis: f32, buttons: u8) -> Input {
        Input { axis, buttons }
    }

    #[test]
    fn frame_clamps_to_action_length() {
        let mut state = State::new(&rules());
        state.action = ActionState { id: 0, frame: 99 };
        assert_eq!(frame(&state, |_| 4), 3);
        assert_eq!(frame(&state, |_| 0), 0);
    }

    #[test]
    fn action_or_phase_change_resets_and_advance_wraps() {
        let rules = rules();
        let mut state = State::new(&rules);
        let frames = |_| 3;
        let first = |_, _| Some(0);
        let second = |_, _| Some(1);

        tick(&mut state, input(0.0, 0), &rules, 2, &frames, &first);
        assert_eq!((state.action.id, state.action.frame), (0, 1));
        tick(&mut state, input(0.0, 0), &rules, 2, &frames, &first);
        assert_eq!((state.action.id, state.action.frame), (0, 2));
        tick(&mut state, input(0.0, 0), &rules, 2, &frames, &first);
        assert_eq!((state.action.id, state.action.frame), (0, 0));
        tick(&mut state, input(0.0, 0), &rules, 2, &frames, &first);
        assert_eq!((state.action.id, state.action.frame), (0, 1));

        tick(&mut state, input(0.0, 0), &rules, 2, &frames, &second);
        assert_eq!((state.action.id, state.action.frame), (1, 0));
    }

    #[test]
    fn empty_catalog_is_a_no_op() {
        let rules = rules();
        let mut state = State::new(&rules);
        let before = state.clone();
        let zero = |_| 0;
        let idle = |_, _| Some(0);
        tick(&mut state, input(0.5, 0), &rules, 0, &zero, &idle);
        assert_eq!(state, before);
    }

    #[test]
    fn out_of_range_selection_falls_back_to_clamped_action() {
        let rules = rules();
        let mut state = State::new(&rules);
        state.action.id = 9;
        let two = |_| 2;
        let out_of_range = |_, _| Some(7);
        tick(&mut state, input(0.0, 0), &rules, 2, &two, &out_of_range);
        assert_eq!(state.action.id, 1);
        assert_eq!(state.action.frame, 0);
    }
}
