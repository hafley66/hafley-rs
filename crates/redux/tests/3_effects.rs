use redux::{Lens, Slice, Zoom, reduce_then_apply};

#[derive(Debug, PartialEq)]
struct State {
    value: i32,
}

#[derive(Debug, PartialEq)]
enum Effect {
    Add(i32),
}

struct Step;

impl Slice for Step {
    type Context<'a> = ();
    type State = State;
    type Event = i32;
    type Output = i32;
    type Effect = Effect;

    fn reduce(
        state: &mut State,
        event: i32,
        _: Self::Context<'_>,
        effects: &mut impl FnMut(Effect),
    ) -> i32 {
        state.value += event;
        effects(Effect::Add(10));
        effects(Effect::Add(20));
        state.value += 100;
        state.value
    }
}

#[test]
fn reduction_completes_before_effect_application_and_reuses_buffer() {
    let mut state = State { value: 0 };
    let mut effects = vec![Effect::Add(999)];
    effects.reserve(2);
    let capacity = effects.capacity();
    let mut observed = Vec::new();

    let output = reduce_then_apply::<Step, _>(&mut state, 3, (), &mut effects, |state, effect| {
        let Effect::Add(amount) = effect;
        observed.push((state.value, effect));
        state.value += amount;
    });

    assert_eq!(output, 103);
    assert_eq!(
        observed,
        vec![(103, Effect::Add(10)), (113, Effect::Add(20))]
    );
    assert_eq!(state.value, 133);
    assert!(effects.is_empty());
    assert_eq!(effects.capacity(), capacity);
}

#[derive(Debug, PartialEq)]
struct Outer {
    inner: State,
    applied: u8,
}

#[derive(Clone, Copy)]
struct InnerLens;

impl Lens<Outer, State> for InnerLens {
    fn get(outer: &Outer) -> &State {
        &outer.inner
    }

    fn get_mut(outer: &mut Outer) -> &mut State {
        &mut outer.inner
    }
}

type FocusedStep = Zoom<InnerLens, Step, Outer>;

#[test]
fn focused_reduction_can_apply_effects_to_outer_state() {
    let mut outer = Outer {
        inner: State { value: 0 },
        applied: 0,
    };
    let mut effects = Vec::new();

    reduce_then_apply::<FocusedStep, _>(&mut outer, 1, (), &mut effects, |outer, effect| {
        let Effect::Add(amount) = effect;
        outer.applied += 1;
        outer.inner.value += amount;
    });

    assert_eq!(
        outer,
        Outer {
            inner: State { value: 131 },
            applied: 2
        }
    );
}
