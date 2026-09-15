//! Macro-free statig qualification inside the existing Slice/Then boundary.
//! Lab policy only: one pending press, inclusive deadline, expiry before eligibility.
//! World owns the machine; borrowed rules and effect sink live for one dispatch.
#![cfg(feature = "serde")]

use redux::{Slice, Then};
use serde::{Deserialize, Serialize};
use statig::blocking::{self, IntoStateMachine, StateMachine};
use statig::{
    Outcome,
    Outcome::{Handled, Super, Transition},
};

#[derive(Clone, Copy)]
struct Input {
    tick: u32,
    press: bool,
    eligible: bool,
    cancel: bool,
}
#[derive(Clone, Copy)]
struct Rules {
    window: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum Phase {
    Empty,
    Pending { deadline: u32 },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
struct Buffer {
    entries: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
enum Effect {
    Consumed(u32),
    Expired(u32),
    Cancelled(u32),
}
struct Context<'a> {
    rules: &'a Rules,
    emit: &'a mut dyn FnMut(Effect),
}
enum Parent {
    Active,
}

impl IntoStateMachine for Buffer {
    type State = Phase;
    type Superstate<'a> = Parent;
    type Event<'a> = Input;
    type Context<'a> = Context<'a>;
    fn initial() -> Phase {
        Phase::Empty
    }
}

impl blocking::State<Buffer> for Phase {
    fn call_handler(&mut self, _: &mut Buffer, ev: &Input, cx: &mut Context<'_>) -> Outcome<Self> {
        if ev.cancel {
            return Super;
        }
        if ev.press {
            if ev.eligible {
                (cx.emit)(Effect::Consumed(ev.tick));
                return Transition(Phase::Empty);
            }
            return Transition(Phase::Pending {
                deadline: ev.tick.checked_add(cx.rules.window).unwrap(),
            });
        }
        if let Phase::Pending { deadline } = self {
            if ev.tick > *deadline {
                (cx.emit)(Effect::Expired(ev.tick));
                return Transition(Phase::Empty);
            }
            if ev.eligible {
                (cx.emit)(Effect::Consumed(ev.tick));
                return Transition(Phase::Empty);
            }
        }
        Handled
    }
    fn call_entry_action(&mut self, storage: &mut Buffer, _: &mut Context<'_>) {
        storage.entries += 1;
    }
    fn superstate(&mut self) -> Option<Parent> {
        Some(Parent::Active)
    }
}

impl blocking::Superstate<Buffer> for Parent {
    fn call_handler(&mut self, _: &mut Buffer, ev: &Input, cx: &mut Context<'_>) -> Outcome<Phase> {
        if ev.cancel {
            (cx.emit)(Effect::Cancelled(ev.tick));
            Transition(Phase::Empty)
        } else {
            Handled
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
struct World {
    input: StateMachine<Buffer>,
    steps: u32,
}

struct InputChart;
impl Slice for InputChart {
    type Context<'a> = &'a Rules;
    type State = World;
    type Event = Input;
    type Output = ();
    type Effect = Effect;
    fn reduce(world: &mut World, ev: Input, rules: &Rules, fx: &mut impl FnMut(Effect)) {
        world
            .input
            .handle_with_context(&ev, &mut Context { rules, emit: fx });
    }
}

struct CountStep;
impl Slice for CountStep {
    type Context<'a> = &'a Rules;
    type State = World;
    type Event = ();
    type Output = u32;
    type Effect = Effect;
    fn reduce(world: &mut World, _: (), _: &Rules, _: &mut impl FnMut(Effect)) -> u32 {
        world.steps += 1;
        world.steps
    }
}

type Step = Then<InputChart, CountStep>;

#[test]
fn serialized_restore_reenters_state_unlike_clone_restore() {
    let rules = Rules { window: 3 };
    let mut original = World::default();
    Step::reduce(
        &mut original,
        Input {
            tick: 0,
            press: true,
            eligible: false,
            cancel: false,
        },
        &rules,
        &mut |_| {},
    );
    let mut cloned = original.clone();
    let mut decoded: World =
        serde_json::from_slice(&serde_json::to_vec(&original).unwrap()).unwrap();
    let next = Input {
        tick: 1,
        press: false,
        eligible: false,
        cancel: false,
    };
    for world in [&mut cloned, &mut decoded] {
        Step::reduce(world, next, &rules, &mut |_| {});
    }
    assert_eq!(cloned.input.state(), decoded.input.state());
    assert_eq!(
        (cloned.input.inner().entries, decoded.input.inner().entries),
        (2, 3)
    );
    assert!(
        cloned != decoded,
        "raw Serde restore is not an exact rollback snapshot"
    );
}

#[test]
fn policies_expiry_consumption_cancellation_and_same_tick_output() {
    let tape = [
        (true, false, false),
        (false, false, false),
        (false, true, false),
        (false, true, false),
        (true, false, false),
        (false, true, true),
    ];
    let mut results = Vec::new();
    for window in [0, 2] {
        let mut world = World::default();
        let mut effects = Vec::new();
        let mut states = Vec::new();
        for (tick, (press, eligible, cancel)) in tape.into_iter().enumerate() {
            let count = Step::reduce(
                &mut world,
                Input {
                    tick: tick as u32,
                    press,
                    eligible,
                    cancel,
                },
                &Rules { window },
                &mut |fx| effects.push(fx),
            );
            assert_eq!(count, tick as u32 + 1);
            states.push(world.input.state().clone());
        }
        results.push((states, effects));
    }
    assert_eq!(
        results,
        vec![
            (
                vec![
                    Phase::Pending { deadline: 0 },
                    Phase::Empty,
                    Phase::Empty,
                    Phase::Empty,
                    Phase::Pending { deadline: 4 },
                    Phase::Empty
                ],
                vec![Effect::Expired(1), Effect::Cancelled(5)]
            ),
            (
                vec![
                    Phase::Pending { deadline: 2 },
                    Phase::Pending { deadline: 2 },
                    Phase::Empty,
                    Phase::Empty,
                    Phase::Pending { deadline: 6 },
                    Phase::Empty
                ],
                vec![Effect::Consumed(2), Effect::Cancelled(5)]
            ),
        ]
    );
}

#[test]
fn clone_restore_replays_pending_state_and_effects_without_reinitializing() {
    let rules = Rules { window: 3 };
    let mut world = World::default();
    Step::reduce(
        &mut world,
        Input {
            tick: 0,
            press: true,
            eligible: false,
            cancel: false,
        },
        &rules,
        &mut |_| {},
    );
    let saved = world.clone();
    let run = |world: &mut World| {
        let mut effects = Vec::new();
        for tick in 1..=4 {
            Step::reduce(
                world,
                Input {
                    tick,
                    press: false,
                    eligible: tick >= 2,
                    cancel: false,
                },
                &rules,
                &mut |fx| effects.push(fx),
            );
        }
        effects
    };
    let expected = run(&mut world);
    let mut restored = saved;
    assert_eq!(run(&mut restored), expected);
    assert!(world == restored);
    assert_eq!(expected, vec![Effect::Consumed(2)]);
    assert_eq!(world.input.inner().entries, 3);
}
