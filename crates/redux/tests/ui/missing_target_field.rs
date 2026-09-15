use redux::machine;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum State {
    Idle,
    Open { token: u32 },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Event {
    Go { token: u32 },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Outcome {
    Handled,
    Unhandled,
}

#[derive(Copy, Clone)]
struct Cx;

fn go(_st: &mut State, _cx: Cx, _ev: &Event, _fx: &mut impl FnMut(())) {}

machine! {
    MissingField for State {
        context: Cx,
        event: Event,
        effect: (),
        output: Outcome { handled: Handled, unhandled: Unhandled },
        reduce(st, ev, cx, fx)
        states: [Idle, Open],
        graph: { nodes: NODES, edges: EDGES },
        ([Idle] [Go { token: _ }] action: go -> [Open], id: GO),
    }
}

fn main() {}
