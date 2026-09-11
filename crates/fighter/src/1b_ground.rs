//! Grounded permission/transition slice consumed by the live Redux controller.
//! Numeric thresholds, integration and entry impulses stay in `2_advance.rs`.
//! Jump edges follow ftCo_Wait/Turn_IASA and fn_800CAF78 in Dash/Run/RunBrake/
//! TurnRun. The crouch lifecycle follows ftCo_Squat/SquatWait/SquatRv. Other
//! guards preserve the existing lab policy, not exact PM timing.

use crate::Phase;
use statig::Outcome::{self, Handled, Transition};
use statig::blocking::{self, IntoStateMachine, IntoStateMachineExt};

/// Caller-resolved facts for one locomotion callback, borrowed for one dispatch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub dash: bool,
    pub walk: bool,
    pub forward: bool,
    pub reverse: bool,
    pub down: bool,
    pub finished: bool,
    pub stopped: bool,
}

pub enum Event {
    JumpRequest,
    GroundIntent(Facts),
    Motion(Facts),
}

struct Ground;

impl IntoStateMachine for Ground {
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

impl blocking::State<Ground> for Phase {
    fn call_handler(&mut self, _: &mut Ground, event: &Event, _: &mut bool) -> Outcome<Phase> {
        use Phase::*;
        match event {
            // Jump wins before the grounded movement callback. Landing recovery
            // and an already-running jumpsquat do not accept another ground jump.
            // ftCo_Squat/SquatWait/SquatRv IASA all expose ftCo_Jump_CheckInput.
            Event::JumpRequest => match self {
                Idle | Walk | Dash | Run | Brake | Turn
                | CrouchEnter | CrouchHold | CrouchExit => Transition(Squat),
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
        }
    }
    fn superstate(&mut self) -> Option<core::convert::Infallible> {
        None
    }
}

impl blocking::Superstate<Ground> for core::convert::Infallible {
    fn call_handler(&mut self, _: &mut Ground, _: &Event, _: &mut bool) -> Outcome<Phase> {
        match *self {}
    }
}

/// Redux owns the serialized phase between dispatches. Rehydrate that logical
/// state into a stack-local statig machine, dispatch once, return its decision.
/// There are no entry/exit hooks, allocations or persistent parallel chart state.
/// `Some(current)` reports a self-transition; `None` preserves the phase clock.
#[tracing::instrument(target = "game_fighter::ground", level = "trace", skip_all)]
pub fn decide(phase: Phase, event: Event) -> Option<Phase> {
    let mut machine = Ground.uninitialized_state_machine();
    *machine.state_mut() = phase;
    let mut changed = false;
    let mut machine = machine.init_with_context(&mut changed);
    machine.handle_with_context(&event, &mut changed);
    changed.then(|| *machine.state())
}
