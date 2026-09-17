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

machine! {
    EventBindingScopes for State {
        context: Cx,
        event: Event,
        effect: (),
        output: Outcome { handled: Handled, unhandled: Unhandled },
        reduce(st, ev, cx, fx)
        states: [Idle, Open],
        graph: { nodes: NODES, edges: EDGES },
        // `token` is an event-pattern binding; it must scope into the target.
        ([Idle] [Go { token }] -> [Open { token: *token }], id: GO),
    }
}

fn main() {}
