//! The shared fighter locomotion statechart.
//!
//! Redux owns the serialized [`Phase`] between dispatches. This module owns
//! the one transient Statig machine and one event sum for grounded and
//! airborne callbacks. Numeric physics and entry effects remain in
//! [`crate::_2_advance`].

use crate::Phase;
use statig::Outcome::{self, Handled, Transition};
use statig::blocking::{self, IntoStateMachine, IntoStateMachineExt};

/// Caller-resolved facts for one grounded locomotion callback.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GroundFacts {
    pub dash: bool,
    pub walk: bool,
    pub forward: bool,
    pub reverse: bool,
    pub down: bool,
    pub finished: bool,
    pub stopped: bool,
}

/// Caller-resolved facts for one airborne locomotion callback.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AirFacts {
    /// Post-integration vertical velocity is `<= 0`.
    pub descending: bool,
    /// Jump button pressed edge for this tick.
    pub jump_pressed: bool,
    /// Remaining air jumps before this dispatch.
    pub jumps_left: u8,
}

/// One event sum consumed by the shared statechart.
pub enum Event {
    JumpRequest,
    GroundIntent(GroundFacts),
    Motion(GroundFacts),
    AirMotion(AirFacts),
    Land,
}

struct Chart;

impl IntoStateMachine for Chart {
    type Event<'a> = Event;
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

impl blocking::State<Chart> for Phase {
    fn call_handler(&mut self, _: &mut Chart, event: &Event, _: &mut bool) -> Outcome<Phase> {
        use Phase::*;
        match event {
            // Jump wins before the grounded movement callback. Landing recovery
            // and an already-running jumpsquat do not accept another ground jump.
            // ftCo_Squat/SquatWait/SquatRv IASA all expose ftCo_Jump_CheckInput.
            Event::JumpRequest => match self {
                Idle | Walk | Dash | Run | Brake | Turn | CrouchEnter | CrouchHold | CrouchExit => {
                    Transition(Squat)
                }
                _ => Handled,
            },
            Event::GroundIntent(f) => match self {
                // ftCo_Wait_IASA:66 -> ftCo_800D5FB0 -> ftCo_Squat_Enter
                // (ftCo_Squat_CheckInput, ftCo_Squat.c:47-81): down enters Squat.
                Idle | Dash | Run if f.down => Transition(CrouchEnter),
                Idle | Dash | Run if f.dash => Transition(Dash),
                Idle if f.walk => Transition(Walk),
                _ => Handled,
            },
            Event::Motion(f) => match self {
                Walk if f.dash => Transition(Dash),
                Walk if !f.walk => Transition(Idle),
                // Self-transition is significant: dash dance restarts the age.
                Dash if f.reverse => Transition(Dash),
                Dash if f.forward && f.finished => Transition(Run),
                Dash if !f.forward => Transition(Brake),
                Run if f.reverse => Transition(Turn),
                Run if !f.walk => Transition(Brake),
                Run if f.down => Transition(CrouchEnter),
                Brake if f.stopped => Transition(Idle),
                Turn if f.reverse => Transition(Dash),
                Turn if f.finished => Transition(Idle),
                // ftCo_Squat_Anim:85-86 -> ftCo_800D638C -> ftCo_MS_SquatWait.
                CrouchEnter if f.finished => Transition(CrouchHold),
                // ftCo_SquatWait_IASA:121 -> ftCo_SquatRv_CheckInput.
                CrouchHold if !f.down => Transition(CrouchExit),
                // ftCo_SquatRv_Anim:59-64 -> ft_8008A2BC -> ftCo_MS_Wait.
                CrouchExit if f.finished => Transition(Idle),
                Landing if f.finished => Transition(Idle),
                Squat if f.finished => Transition(Jump),
                _ => Handled,
            },
            // The jump callback runs before gravity, so a pressed jump with
            // budget beats a competing descending fact.
            Event::AirMotion(f) => match self {
                Jump | AirJump if f.jump_pressed && f.jumps_left > 0 => Transition(AirJump),
                Jump | AirJump if f.descending => Transition(Fall),
                Fall if f.jump_pressed && f.jumps_left > 0 => Transition(AirJump),
                _ => Handled,
            },
            // Contact is already resolved by the caller; only airborne phases land.
            Event::Land => match self {
                Jump | AirJump | Fall => Transition(Landing),
                _ => Handled,
            },
        }
    }

    fn superstate(&mut self) -> Option<core::convert::Infallible> {
        None
    }
}

impl blocking::Superstate<Chart> for core::convert::Infallible {
    fn call_handler(&mut self, _: &mut Chart, _: &Event, _: &mut bool) -> Outcome<Phase> {
        match *self {}
    }
}

/// Rehydrate the Redux-owned phase into the shared stack-local Statig
/// machine, dispatch once, and return its transition decision.
///
/// `Some(current)` reports a self-transition; `None` preserves the phase
/// clock. There are no entry/exit hooks, allocations or persistent parallel
/// chart state.
#[tracing::instrument(target = "game_fighter::chart", level = "trace", skip_all)]
pub fn decide(phase: Phase, event: Event) -> Option<Phase> {
    let mut machine = Chart.uninitialized_state_machine();
    *machine.state_mut() = phase;
    let mut changed = false;
    let mut machine = machine.init_with_context(&mut changed);
    machine.handle_with_context(&event, &mut changed);
    changed.then(|| *machine.state())
}
