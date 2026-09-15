use redux::{Lens, Never, Slice, Then, Zoom, slice};

#[derive(Copy, Clone)]
struct Context<'a> {
    bias: &'a i32,
    factor: i32,
}

slice! {
    AddBias for i32 {
        context: Context<'a>, event: i32, output: i32,
        reduce(state, event, cx, _effects) {
            *state += event + *cx.bias;
            *state
        }
    }
}

slice! {
    Multiply for i32 {
        context: Context<'a>, event: i32, output: i32,
        reduce(state, event, cx, _effects) {
            *state *= cx.factor;
            event * cx.factor
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
struct AppState {
    counter: i32,
    untouched: i32,
}

#[derive(Copy, Clone)]
struct Counter;

impl Lens<AppState, i32> for Counter {
    fn get(outer: &AppState) -> &i32 {
        &outer.counter
    }

    fn get_mut(outer: &mut AppState) -> &mut i32 {
        &mut outer.counter
    }
}

#[test]
fn redux_alone_defines_contextual_tuple_then_and_zoom_pipeline() {
    type TuplePipeline = Zoom<Counter, (AddBias, Multiply), AppState>;
    type ThenPipeline = Then<AddBias, Multiply>;

    let bias = 2;
    let cx = Context {
        bias: &bias,
        factor: 3,
    };
    let mut app = AppState {
        counter: 1,
        untouched: 99,
    };
    let output = TuplePipeline::reduce(&mut app, 4, cx, &mut |never: Never| match never {});
    assert_eq!(
        app,
        AppState {
            counter: 21,
            untouched: 99
        }
    );
    assert_eq!(output, 21);
    assert_eq!(Counter::get(&app), &21);

    let mut direct = 1;
    let direct_output =
        ThenPipeline::reduce(&mut direct, 4, cx, &mut |never: Never| match never {});
    assert_eq!(direct, app.counter);
    assert_eq!(direct_output, output);
    assert_eq!(core::mem::size_of::<TuplePipeline>(), 0);
}
