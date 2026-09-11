//! Airborne permission/transition slice consumed by the live Redux controller.
//! Numeric thresholds, integration, air drift, double-jump impulse and landing
//! reset stay in `_2_advance.rs`; this chart selects destination only.
//!
//! Migrated procedural writes from `_2_advance.rs`:
//!
//! ```text
//! Jump    --descending-->                  Fall
//! AirJump --descending-->                  Fall
//! Jump    --jump_pressed && jumps_left>0--> AirJump
//! Fall    --jump_pressed && jumps_left>0--> AirJump
//! airborne --caller-resolved contact-->    Landing
//! ```
//!
//! Guard order preserves the current callback execution order: the pre-gravity
//! jump check runs before the post-gravity fall write, so a pressed jump with a
//! remaining jump wins over `descending` in the competing case. `Land` is only
//! selected for airborne phases and only when the caller has already resolved a
//! descending ground contact; the chart does not read position or velocity.
//!
//! No entry/exit hooks, allocation or parallel phase field. Physics is untouched.

use crate::Phase;
use statig::Outcome::{self, Handled, Transition};
use statig::blocking::{self, IntoStateMachine, IntoStateMachineExt};

/// Caller-resolved facts for one air callback, borrowed for one dispatch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AirFacts {
    /// Post-integration vertical velocity is `<= 0`.
    pub descending: bool,
    /// Jump button pressed edge for this tick.
    pub jump_pressed: bool,
    /// Remaining air jumps before this dispatch.
    pub jumps_left: u8,
}

pub enum AirEvent {
    Motion(AirFacts),
    Land,
}

struct Air;

impl IntoStateMachine for Air {
    type Event<'a> = AirEvent;
    type Context<'a> = bool;
    type State = Phase;
    type Superstate<'a> = core::convert::Infallible;
    fn initial() -> Phase {
        Phase::Idle
    }
    fn after_transition(&mut self, _: &Phase, _: &Phase, transitioned: &mut bool) {
        *transitioned = true;
    }
}

impl blocking::State<Air> for Phase {
    fn call_handler(&mut self, _: &mut Air, event: &AirEvent, _: &mut bool) -> Outcome<Phase> {
        use Phase::*;
        match event {
            // Pre-gravity jump check wins over the post-gravity fall write, so a
            // pressed jump with budget beats a competing descending fact.
            AirEvent::Motion(f) => match self {
                Jump | AirJump if f.jump_pressed && f.jumps_left > 0 => Transition(AirJump),
                Jump | AirJump if f.descending => Transition(Fall),
                Fall if f.jump_pressed && f.jumps_left > 0 => Transition(AirJump),
                _ => Handled,
            },
            // Contact is already resolved by the caller; only airborne phases land.
            AirEvent::Land => match self {
                Jump | AirJump | Fall => Transition(Landing),
                _ => Handled,
            },
        }
    }
    fn superstate(&mut self) -> Option<core::convert::Infallible> {
        None
    }
}

impl blocking::Superstate<Air> for core::convert::Infallible {
    fn call_handler(&mut self, _: &mut Air, _: &AirEvent, _: &mut bool) -> Outcome<Phase> {
        match *self {}
    }
}

/// Redux owns the serialized phase between dispatches. Rehydrate that logical
/// state into a stack-local statig machine, dispatch once, return its decision.
/// `Some(current)` reports a self-transition; `None` preserves the phase clock.
#[tracing::instrument(target = "game_fighter::air", level = "trace", skip_all)]
pub fn decide(phase: Phase, event: AirEvent) -> Option<Phase> {
    let mut machine = Air.uninitialized_state_machine();
    *machine.state_mut() = phase;
    let mut changed = false;
    let mut machine = machine.init_with_context(&mut changed);
    machine.handle_with_context(&event, &mut changed);
    changed.then(|| *machine.state())
}
