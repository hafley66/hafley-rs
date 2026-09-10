//! S2 first executable common-ground chart: source-resolved crouch lifecycle.
//!
//! Bounded qualification. The only transitions implemented are the four edges
//! the Melee decomp common callbacks resolve for the crouch cycle:
//!
//! ```text
//! Idle --crouch_request--> CrouchEnter --animation_finished--> CrouchHold
//! CrouchHold --crouch_release--> CrouchExit --animation_finished--> Idle
//! ```
//!
//! The chart consumes semantic facts only. Buttons, stick positions, velocities
//! and thresholds belong to the caller that resolves these booleans; the chart
//! owns permission and destination selection, not input decoding.
//!
//! Source edges (read-only decomp under `kneeman-lines/4_melee_decomp`):
//!
//! - `Idle` = `ftCo_MS_Wait`. `ftCo_Wait_IASA` line 66 calls
//!   `ftCo_800D5FB0`; that helper runs `ftCo_Squat_CheckInput` and then
//!   `ftCo_Squat_Enter` (`ftCo_Wait.c:66`, `ftCo_Squat.c:47-81`).
//! - `CrouchEnter` = `ftCo_MS_Squat`. `ftCo_Squat_Anim` finishes when
//!   `!ftAnim_IsFramesRemaining` and calls `ftCo_800D638C`, which enters
//!   `ftCo_MS_SquatWait` (`ftCo_Squat.c:83-88`, `ftCo_SquatWait.c:94-97`).
//! - `CrouchHold` = `ftCo_MS_SquatWait`. `ftCo_SquatWait_IASA` line 121 calls
//!   `ftCo_SquatRv_CheckInput`, which runs `ftCo_SquatRv_Enter` on release
//!   (`ftCo_SquatWait.c:121`, `ftCo_SquatRv.c:42-57`).
//! - `CrouchExit` = `ftCo_MS_SquatRv`. `ftCo_SquatRv_Anim` finishes and calls
//!   `ft_8008A2BC` -> `ft_8008A348` ->
//!   `Fighter_ChangeMotionState(gobj, ftCo_MS_Wait, ...)`
//!   (`ftCo_SquatRv.c:59-64`, `ft/ft_08A1.c:56-99`).
//!
//! Hard facts:
//! - `crouch_request` is `ftCo_Squat_CheckInput` (`ftCo_Squat.c:47-55`).
//! - `crouch_release` is `ftCo_SquatRv_CheckInput` (`ftCo_SquatRv.c:42-51`).
//! - `animation_finished` is `!ftAnim_IsFramesRemaining` for the current action.
//!
//! Deliberately unsupported in this qualification (no unconditional edges are
//! invented for them). Every interrupt check appearing before the crouch edges
//! in the source IASA lists is out of scope:
//! `ftCo_SpecialS_CheckInput`, `ftCo_Attack100_CheckInput`, `ftCo_800D6824`,
//! `ftCo_800D68C0`, `ftCo_Catch_CheckInput`, `ftCo_AttackS4_CheckInput`,
//! `ftCo_AttackHi4_CheckInput`, `ftCo_AttackLw4_CheckInput`,
//! `ftCo_AttackS3_CheckInput`, `ftCo_AttackHi3_CheckInput`,
//! `ftCo_AttackLw3_CheckInput`, `ftCo_Attack1_CheckInput`, `ftCo_80091A4C`,
//! `ftCo_800DE9D8`, `ftCo_Jump_CheckInput`, `ftCo_80099F9C`,
//! `ftCo_Squat_IASA_inline` (`ftCo_8009A228`), `ftCo_Dash_CheckInput`,
//! `ftCo_Walk_CheckInput`. `ftCo_SquatWait_CheckInput`/`fn_800D62C4` (the
//! held-stick re-enter) is likewise unmodeled.
//!
//! No entry or exit actions are defined, so statig's Serde re-initialization
//! cannot mutate logical state. Logical state is the one statig state; there is
//! no parallel phase field. No universal Melee/PM policy is claimed.

use redux::{Never, Slice};
use serde::{Deserialize, Serialize};
use statig::Outcome::Transition;
use statig::blocking::{self, IntoStateMachine, StateMachine};

/// Semantic facts resolved by the caller for one dispatch. Immutable for the call.
/// This qualification does not schedule the source game's callback phases.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facts {
    /// `!ftAnim_IsFramesRemaining(gobj)` for the current action.
    pub animation_finished: bool,
    /// `ftCo_Squat_CheckInput` (down past `p_ftCommonData->x90`).
    pub crouch_request: bool,
    /// `ftCo_SquatRv_CheckInput` (stick above `-p_ftCommonData->x94`).
    pub crouch_release: bool,
}

/// The one primary action in the source-resolved crouch lifecycle. This enum is
/// the statig state; transitions are executed by statig, not by a parallel graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// `ftCo_MS_Wait`.
    Idle,
    /// `ftCo_MS_Squat`.
    CrouchEnter,
    /// `ftCo_MS_SquatWait`.
    CrouchHold,
    /// `ftCo_MS_SquatRv`.
    CrouchExit,
}

/// Shared storage for the chart. Unit: the machine owns no mutable facts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Chart;

impl IntoStateMachine for Chart {
    type Event<'a> = Facts;
    type Context<'a> = ();
    type State = Action;
    type Superstate<'sub> = core::convert::Infallible;

    fn initial() -> Action {
        Action::Idle
    }
}

impl blocking::State<Chart> for Action {
    fn call_handler(
        &mut self,
        _: &mut Chart,
        ev: &Facts,
        _: &mut (),
    ) -> statig::Outcome<Action> {
        match *self {
            // ftCo_Wait_IASA:66 -> ftCo_800D5FB0 -> ftCo_Squat_Enter.
            Action::Idle if ev.crouch_request => Transition(Action::CrouchEnter),
            // ftCo_Squat_Anim:83-88 -> ftCo_800D638C -> ftCo_MS_SquatWait.
            // Squat has no crouch-release interrupt, so completion is checked
            // first and crouch_request/crouch_release are rejected here.
            Action::CrouchEnter if ev.animation_finished => Transition(Action::CrouchHold),
            // ftCo_SquatWait_IASA:121 -> ftCo_SquatRv_CheckInput -> SquatRv.
            Action::CrouchHold if ev.crouch_release => Transition(Action::CrouchExit),
            // ftCo_SquatRv_Anim:59-64 -> ft_8008A2BC -> ftCo_MS_Wait.
            Action::CrouchExit if ev.animation_finished => Transition(Action::Idle),
            _ => statig::Outcome::Handled,
        }
    }
    fn superstate(&mut self) -> Option<core::convert::Infallible> {
        None
    }
}

impl blocking::Superstate<Chart> for core::convert::Infallible {
    fn call_handler(
        &mut self,
        _: &mut Chart,
        _: &Facts,
        _: &mut (),
    ) -> statig::Outcome<Action> {
        match *self {}
    }
}

/// The chart owns exactly one statig machine; logical state is `machine.state()`.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct CrouchChart {
    machine: StateMachine<Chart>,
}

impl CrouchChart {
    /// Current primary action.
    pub fn action(&self) -> Action {
        *self.machine.state()
    }

    /// Dispatch immutable facts and return the resulting action.
    #[tracing::instrument(target = "game_fighter::chart", level = "trace", skip_all)]
    pub fn step(&mut self, facts: Facts) -> Action {
        self.machine.handle(&facts);
        *self.machine.state()
    }
}

// Logical equality is the primary action. `StateMachine` also carries statig's
// runtime `initialized` bit, which Serde intentionally resets; the chart defines
// no entry/exit side effects, so that bit is not logical state.
impl PartialEq for CrouchChart {
    fn eq(&self, other: &Self) -> bool {
        self.action() == other.action()
    }
}

impl std::fmt::Debug for CrouchChart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.action().fmt(f)
    }
}

/// Redux boundary: dispatch [`Facts`] through the chart. Pure, no effects.
pub struct CrouchSlice;

impl Slice for CrouchSlice {
    type Context<'a> = ();
    type State = CrouchChart;
    type Event = Facts;
    type Output = Action;
    type Effect = Never;

    fn reduce(
        st: &mut Self::State,
        ev: Self::Event,
        _cx: Self::Context<'_>,
        _fx: &mut impl FnMut(Self::Effect),
    ) -> Self::Output {
        st.step(ev)
    }
}
