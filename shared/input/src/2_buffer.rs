//! One intent lane, with V1 age-before-record and newest-press overwrite semantics.
//! The game chooses whether to call advance, eligibility, duration and cancellation.
//! No entry/exit actions: statig's deserialization reinitialization is inert.
use serde::{Deserialize, Serialize};
use statig::Outcome::{Handled, Super, Transition};
use statig::blocking::{self, IntoStateMachine, StateMachine};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum Phase {
    Empty,
    Pending { remaining: u32 },
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Machine;
enum Parent {
    Active,
}
struct Event {
    press: bool,
    eligible: bool,
    cancel: bool,
    window: u32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Outcome {
    #[default]
    None,
    Consumed,
    Expired,
    Cancelled,
}

impl IntoStateMachine for Machine {
    type State = Phase;
    type Superstate<'a> = Parent;
    type Event<'a> = Event;
    type Context<'a> = Outcome;
    fn initial() -> Phase {
        Phase::Empty
    }
}
impl blocking::State<Machine> for Phase {
    fn call_handler(
        &mut self,
        _: &mut Machine,
        ev: &Event,
        out: &mut Outcome,
    ) -> statig::Outcome<Self> {
        if ev.cancel {
            return Super;
        }
        let prior = match self {
            Phase::Empty => None,
            Phase::Pending { remaining } => Some(*remaining),
        };
        let remaining = if ev.press {
            Some(ev.window)
        } else {
            prior.and_then(|n| n.checked_sub(1))
        };
        match remaining {
            Some(_) if ev.eligible => {
                *out = Outcome::Consumed;
                Transition(Phase::Empty)
            }
            Some(remaining) => Transition(Phase::Pending { remaining }),
            None if prior.is_some() => {
                *out = Outcome::Expired;
                Transition(Phase::Empty)
            }
            None => Handled,
        }
    }
    fn superstate(&mut self) -> Option<Parent> {
        Some(Parent::Active)
    }
}
impl blocking::Superstate<Machine> for Parent {
    fn call_handler(
        &mut self,
        _: &mut Machine,
        _: &Event,
        out: &mut Outcome,
    ) -> statig::Outcome<Phase> {
        *out = Outcome::Cancelled;
        Transition(Phase::Empty)
    }
}

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Buffer {
    machine: StateMachine<Machine>,
}
impl std::fmt::Debug for Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.machine.state().fmt(f)
    }
}
impl Buffer {
    pub fn advance(&mut self, press: bool, eligible: bool, cancel: bool, window: u32) -> Outcome {
        let mut out = Outcome::None;
        self.machine.handle_with_context(
            &Event {
                press,
                eligible,
                cancel,
                window,
            },
            &mut out,
        );
        out
    }
    pub fn remaining(&self) -> Option<u32> {
        match self.machine.state() {
            Phase::Empty => None,
            Phase::Pending { remaining } => Some(*remaining),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_and_game_owned_freeze() {
        let mut buffer = Buffer::default();
        let mut trace = Vec::new();
        // Releasing/holding the physical button is resolved by the game before dispatch.
        for (press, eligible, cancel) in [
            (true, false, false),
            (false, false, false),
            (true, false, false),
            (false, false, false),
            (false, true, false),
            (false, true, false),
            (true, false, false),
            (false, true, true),
        ] {
            let out = buffer.advance(press, eligible, cancel, 2);
            trace.push((out, buffer.remaining()));
        }
        assert_eq!(
            trace,
            vec![
                (Outcome::None, Some(2)),
                (Outcome::None, Some(1)),
                (Outcome::None, Some(2)),
                (Outcome::None, Some(1)),
                (Outcome::Consumed, None),
                (Outcome::None, None),
                (Outcome::None, Some(2)),
                (Outcome::Cancelled, None)
            ]
        );
        buffer.advance(true, false, false, 2);
        let frozen = buffer.clone();
        // A game pauses aging by omitting dispatch; inspecting never advances time.
        for _ in 0..100 {
            assert_eq!(buffer.remaining(), Some(2));
        }
        assert_eq!(buffer, frozen);
    }
    #[test]
    fn windows_overwrite_cancel_and_serialized_restore() {
        for window in [0, 2] {
            let mut buffer = Buffer::default();
            assert_eq!(buffer.advance(true, false, false, window), Outcome::None);
            let mut clone = buffer.clone();
            let mut decoded: Buffer =
                serde_json::from_slice(&serde_json::to_vec(&buffer).unwrap()).unwrap();
            for (press, eligible, cancel) in [
                (false, false, false),
                (false, true, false),
                (true, false, false),
                (true, false, false),
                (false, true, true),
                (false, true, false),
            ] {
                let result = buffer.advance(press, eligible, cancel, window);
                assert_eq!(clone.advance(press, eligible, cancel, window), result);
                assert_eq!(decoded.advance(press, eligible, cancel, window), result);
                assert_eq!(buffer, clone);
                assert_eq!(buffer, decoded);
            }
            assert_eq!(buffer.remaining(), None);
        }
    }
    #[test]
    fn exact_expiry_and_single_consumption() {
        for window in 0..=3 {
            for delay in 0..=4 {
                let mut buffer = Buffer::default();
                let mut outcomes = Vec::new();
                for tick in 0..=5 {
                    let out = buffer.advance(tick == 0, tick >= delay, false, window);
                    if out != Outcome::None {
                        outcomes.push(out);
                    }
                }
                assert_eq!(
                    outcomes,
                    vec![if delay <= window {
                        Outcome::Consumed
                    } else {
                        Outcome::Expired
                    }]
                );
            }
        }
    }
}
