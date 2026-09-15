use redux::machine;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum State {
    Awaiting { attempts: u32 },
    Open { token: u32 },
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

machine! {
    SourceBindingEscapes for State {
        context: Cx,
        event: Event,
        effect: (),
        output: Outcome { handled: Handled, unhandled: Unhandled },
        reduce(st, ev, cx, fx)
        states: [Awaiting, Open],
        graph: { nodes: NODES, edges: EDGES },
        // `attempts` is bound inside the state `matches!` and must NOT escape;
        // reading it in the target is an unresolved name.
        ([Awaiting { attempts }] [Go] -> [Open { token: attempts }], id: GO),
    }
}

fn main() {}
