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

// Receipt effects stay in this test's snapshot so full replay output can be
// compared. No external effects are executed during speculative/replayed steps.
struct ChartSim;
impl rollback::RollbackSim for ChartSim {
    type State = (World, Vec<Effect>);
    type Input = u8;
    type Config = Rules;
    fn initial(_: &Rules) -> Self::State {
        (World::default(), Vec::new())
    }
    fn advance(state: &Self::State, inputs: &[u8], rules: &Rules) -> Self::State {
        let (mut world, mut effects) = state.clone();
        assert_eq!(inputs.len(), 1);
        let tick = world.steps;
        let bits = inputs[0];
        Step::reduce(
            &mut world,
            Input {
                tick,
                press: bits & 1 != 0,
                eligible: bits & 2 != 0,
                cancel: bits & 4 != 0,
            },
            rules,
            &mut |fx| effects.push(fx),
        );
        (world, effects)
    }
    fn checksum(state: &Self::State) -> u128 {
        // Test checksum only; snapshots themselves are cloned, never decoded.
        serde_json::to_vec(state)
            .unwrap()
            .into_iter()
            .fold(0u128, |hash, byte| {
                hash.wrapping_mul(257).wrapping_add(u128::from(byte) + 1)
            })
    }
}

#[test]
fn ggrs_requests_restore_chart_and_replay_full_state_and_effects() {
    use rollback::RollbackSim;
    for window in [0, 2] {
        for distance in [1, 7] {
            let rules = Rules { window };
            let mut session = rollback::synctest_session::<ChartSim>(1, distance);
            let mut game = rollback::Game::<ChartSim>::new(rules);
            let mut expected = ChartSim::initial(&rules);
            let mut saved = std::collections::BTreeMap::new();
            let (mut saves, mut loads, mut advances, mut repeated_saves) = (0, 0, 0, 0);
            // Pending, consume/expire, idle, replace, parent cancellation, immediate consume.
            for tick in 0..64 {
                let input = [1, 0, 2, 0, 1, 1, 4, 3][tick % 8];
                session.add_local_input(0, input).unwrap();
                let requests = session.advance_frame().unwrap();
                for request in &requests {
                    match request {
                        ggrs::GgrsRequest::SaveGameState { .. } => saves += 1,
                        ggrs::GgrsRequest::LoadGameState { .. } => loads += 1,
                        ggrs::GgrsRequest::AdvanceFrame { .. } => advances += 1,
                    }
                }
                game.handle_observed(requests, |frame, hash| {
                    if let Some(previous) = saved.insert(frame, hash) {
                        assert_eq!(hash, previous, "saved frame {frame}");
                        repeated_saves += 1;
                    }
                });
                expected = ChartSim::advance(&expected, &[input], &rules);
                assert!(
                    game.state == expected,
                    "full state/effect mismatch: window={window}, distance={distance}, tick={tick}"
                );
            }
            assert!(
                saves > 0 && loads > 0 && advances > 64,
                "must execute actual save/load/replay requests: window={window} distance={distance} saves={saves} loads={loads} advances={advances} repeated_saves={repeated_saves}"
            );
            if distance > 1 {
                assert!(
                    repeated_saves > 0,
                    "multi-frame replay must compare repeated saves"
                );
            }
            assert!(game.state.1.contains(&Effect::Cancelled(6)));
            assert!(game.state.1.contains(&Effect::Consumed(7)));
            assert!(game.state.1.contains(&if window == 0 {
                Effect::Expired(1)
            } else {
                Effect::Consumed(2)
            }));
        }
    }
}

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
fn ggrs_restore_detects_accidental_serde_reinitialization() {
    use rollback::RollbackSim;
    let rules = Rules { window: 3 };
    let mut session = rollback::synctest_session::<ChartSim>(1, 3);
    let mut game = rollback::Game::<ChartSim>::new(rules);
    let mut expected = ChartSim::initial(&rules);
    let mut loads = 0;
    let mut detected = None;
    for tick in 0..16 {
        let input = if tick % 4 == 0 { 1 } else { 0 };
        session.add_local_input(0, input).unwrap();
        for request in session.advance_frame().unwrap() {
            let restoring = matches!(&request, ggrs::GgrsRequest::LoadGameState { .. });
            game.handle(vec![request]);
            if restoring {
                loads += 1;
                // Deliberately broken restore path: GGRS saved an initialized
                // clone, but this extra round-trip discards initialization.
                game.state =
                    serde_json::from_slice(&serde_json::to_vec(&game.state).unwrap()).unwrap();
            }
        }
        expected = ChartSim::advance(&expected, &[input], &rules);
        if game.state != expected {
            detected = Some((
                game.state.0.input.inner().entries,
                expected.0.input.inner().entries,
            ));
            break;
        }
    }
    assert!(loads > 0, "negative test must cross an actual GGRS restore");
    let (actual_entries, expected_entries) = detected.expect("must detect the broken restore");
    assert!(
        actual_entries > expected_entries,
        "restoration reran entry actions"
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
