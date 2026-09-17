use redux::machine;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum State {
    Idle,
    Open,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Event {
    Go,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Outcome {
    Handled,
    Unhandled,
}

#[derive(Copy, Clone)]
struct Cx;

// Wrong signature: the macro passes the event by reference.
fn go(_st: &mut State, _cx: Cx, _ev: Event, _fx: &mut impl FnMut(())) {}

machine! {
    OwnedEvent for State {
        context: Cx,
        event: Event,
        effect: (),
        output: Outcome { handled: Handled, unhandled: Unhandled },
        reduce(st, ev, cx, fx)
        states: [Idle, Open],
        graph: { nodes: NODES, edges: EDGES },
        ([Idle] [Go] action: go -> [Open], id: GO),
    }
}

fn main() {}
