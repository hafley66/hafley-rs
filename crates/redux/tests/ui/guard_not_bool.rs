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

// Wrong return type: guards must return bool.
fn always(_st: &State, _cx: Cx, _ev: &Event) -> u32 {
    1
}

machine! {
    NotBool for State {
        context: Cx,
        event: Event,
        effect: (),
        output: Outcome { handled: Handled, unhandled: Unhandled },
        reduce(st, ev, cx, fx)
        states: [Idle, Open],
        graph: { nodes: NODES, edges: EDGES },
        ([Idle] [Go] guard: always -> [Open], id: GO),
    }
}

fn main() {}
