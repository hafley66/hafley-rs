//! Shared model for the enum-state machine example and its integration tests.
//!
//! Every machine is declared once here; `machine!` expands both the `Slice`
//! dispatch body and the `Node`/`Edge` graph constants from that single row
//! list. Nothing in this module is game-specific.

use core::marker::PhantomData;

use redux::machine;
use redux::{Graph, Lens, Never, Slice, Then, Zoom};
use serde::{Deserialize, Serialize};

/// Shared copy context. `Slice::Context<'a>: Copy`, so it stays a plain struct.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cx {
    pub max_attempts: u32,
}

/// Shared inert effect vocabulary for both machines.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Signal {
    Notify { id: u32 },
    Warn,
    Opened { token: u32 },
    Armed,
}

// ---------------------------------------------------------------------------
// Session machine: enum state with payloads, explicit stay, guard fallback.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    #[default]
    Idle,
    Awaiting {
        attempts: u32,
    },
    Open {
        token: u32,
        frame: u32,
    },
    /// Declared, deliberately isolated: no row names it as source or target.
    Faulted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionEvent {
    Begin {
        id: u32,
    },
    Retry,
    Confirm {
        token: u32,
    },
    Tick,
    Abort,
    Ping,
    /// Declared nowhere: exercises the unhandled result.
    Unknown,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionOutcome {
    Handled,
    Unhandled,
}

fn never(_st: &SessionState, _cx: Cx, _ev: &SessionEvent) -> bool {
    false
}

fn can_retry(st: &SessionState, cx: Cx, _ev: &SessionEvent) -> bool {
    matches!(st, SessionState::Awaiting { attempts } if *attempts < cx.max_attempts)
}

fn begin(_st: &mut SessionState, _cx: Cx, ev: &SessionEvent, fx: &mut impl FnMut(Signal)) {
    if let SessionEvent::Begin { id } = ev {
        fx(Signal::Notify { id: *id });
    }
}

fn retry(st: &mut SessionState, _cx: Cx, _ev: &SessionEvent, _fx: &mut impl FnMut(Signal)) {
    if let SessionState::Awaiting { attempts } = st {
        *attempts += 1;
    }
}

/// Reads the source variant before the target assignment, so the emitted
/// `Notify` carries the pre-transition attempt count.
fn confirm(st: &mut SessionState, _cx: Cx, ev: &SessionEvent, fx: &mut impl FnMut(Signal)) {
    if let SessionState::Awaiting { attempts } = st {
        fx(Signal::Notify { id: *attempts });
    }
    if let SessionEvent::Confirm { token } = ev {
        fx(Signal::Opened { token: *token });
    }
}

fn tick(st: &mut SessionState, _cx: Cx, _ev: &SessionEvent, _fx: &mut impl FnMut(Signal)) {
    if let SessionState::Open { frame, .. } = st {
        *frame += 1;
    }
}

fn ping(_st: &mut SessionState, _cx: Cx, _ev: &SessionEvent, fx: &mut impl FnMut(Signal)) {
    fx(Signal::Warn);
}

fn stop(st: &mut SessionState, _cx: Cx, _ev: &SessionEvent, fx: &mut impl FnMut(Signal)) {
    if matches!(st, SessionState::Open { .. }) {
        fx(Signal::Warn);
    }
}

machine! {
    /// Idle -> Awaiting -> Open with a retry stay, a payload-keyed Confirm
    /// transition, a guard fallback on `Ping`, and an isolated `Faulted` node.
    pub SessionMachine for SessionState {
        context: Cx,
        event: SessionEvent,
        effect: Signal,
        output: SessionOutcome { handled: Handled, unhandled: Unhandled },
        reduce(st, ev, cx, fx)
        states: [Idle, Awaiting, Open, Faulted],
        graph: { nodes: SESSION_NODES, edges: SESSION_EDGES },
        ([Idle] [Begin { id: _ }] action: begin -> [Awaiting { attempts: 1 }], id: BEGIN),
        ([Idle] [Ping] guard: never action: ping -> [stay], id: IDLE_PING_GUARDED),
        ([Idle] [Ping] action: ping -> [stay], id: IDLE_PING_FALLBACK),
        ([Awaiting { attempts: _ }] [Retry] guard: can_retry action: retry -> [stay], id: RETRY),
        ([Awaiting { attempts: _ }] [Confirm { token }] action: confirm -> [Open { token: *token, frame: 0 }], id: CONFIRM),
        ([Awaiting { attempts: _ }] [Abort] action: stop -> [Idle], id: ABORT_AWAITING),
        ([Open { token: _, frame: _ }] [Tick] action: tick -> [stay], id: OPEN_TICK),
        ([Open { token: _, frame: _ }] [Abort] action: stop -> [Idle], id: ABORT_OPEN),
    }
}

/// Declared session graph: rows and nodes from the same macro invocation.
pub const SESSION_GRAPH: Graph = Graph::new(SESSION_NODES, SESSION_EDGES);

// ---------------------------------------------------------------------------
// Gate machine: the second machine in the composed routing preview.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateState {
    #[default]
    Dormant,
    Armed {
        count: u32,
    },
    /// Declared, deliberately isolated.
    Spent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateEvent {
    Arm,
    Fire,
    Nop,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateOutcome {
    Handled,
    Unhandled,
}

fn arm(_st: &mut GateState, _cx: Cx, _ev: &GateEvent, fx: &mut impl FnMut(Signal)) {
    fx(Signal::Armed);
}

fn fire(st: &mut GateState, _cx: Cx, _ev: &GateEvent, _fx: &mut impl FnMut(Signal)) {
    if let GateState::Armed { count } = st {
        *count += 1;
    }
}

machine! {
    /// Dormant -> Armed gate with a counter stay.
    pub GateMachine for GateState {
        context: Cx,
        event: GateEvent,
        effect: Signal,
        output: GateOutcome { handled: Handled, unhandled: Unhandled },
        reduce(st, ev, cx, fx)
        states: [Dormant, Armed, Spent],
        graph: { nodes: GATE_NODES, edges: GATE_EDGES },
        ([Dormant] [Arm] action: arm -> [Armed { count: 0 }], id: ARM),
        ([Dormant] [Nop] -> [stay], id: DORMANT_NOP),
        ([Armed { count: _ }] [Fire] action: fire -> [stay], id: FIRE),
        ([Armed { count: _ }] [Nop] -> [stay], id: ARMED_NOP),
        ([Armed { count: _ }] [Arm] -> [stay], id: ARMED_KEEP),
    }
}

/// Declared gate graph.
pub const GATE_GRAPH: Graph = Graph::new(GATE_NODES, GATE_EDGES);

// ---------------------------------------------------------------------------
// D2 rendering: consumer-side formatting over the library's graph metadata.
// ---------------------------------------------------------------------------

/// Render one declared graph as a standalone D2 document. One node per declared
/// state and one edge per declared row, `[stay]` included as a self edge.
pub fn machine_d2(graph: &Graph, title: &str) -> String {
    let mut out = String::from(
        "direction: right\nclasses: {\n  state: {style: {fill: \"#18303b\"; stroke: \"#65d9e6\"; font-color: \"#e2edf0\"; border-radius: 8}}\n  transition: {style: {stroke: \"#65d9e6\"; font-color: \"#efcf75\"}}\n}\n",
    );
    out.push_str(&format!("\"{title}\": \"{title}\" {{\n"));
    render_d2_prefixed(&mut out, graph, "  ", "");
    out.push_str("}\n");
    out
}

/// Append a graph's node and edge lines to `out` at `indent`, with every node
/// id prefixed so two machines can share one flat document without collisions.
pub fn render_d2_prefixed(out: &mut String, graph: &Graph, indent: &str, prefix: &str) {
    for node in graph.nodes {
        out.push_str(&format!(
            "{indent}\"{prefix}{id}\": \"{prefix}{id}\" {{ class: state }}\n",
            id = node.id
        ));
    }
    for edge in graph.edges {
        // The stable row id leads the label so parallel rows (same source and
        // event) stay distinguishable in the rendered graph.
        let mut label = format!("{}: {}", edge.id, edge.event);
        if let Some(guard) = edge.guard {
            label.push_str(&format!(" [{guard}]"));
        }
        if let Some(action) = edge.action {
            label.push_str(&format!(" {action}()"));
        }
        out.push_str(&format!(
            "{indent}\"{prefix}{source}\" -> \"{prefix}{target}\": \"{label}\" {{ class: transition }}\n",
            source = edge.source,
            target = edge.target,
        ));
    }
}

// ---------------------------------------------------------------------------
// Composed system: Session -> authored router -> Gate through Then and Zoom.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct System {
    pub session: SessionState,
    pub gate: GateState,
}

#[derive(Copy, Clone, Debug)]
pub struct SessionLens;

impl Lens<System, SessionState> for SessionLens {
    fn get(outer: &System) -> &SessionState {
        &outer.session
    }
    fn get_mut(outer: &mut System) -> &mut SessionState {
        &mut outer.session
    }
}

#[derive(Copy, Clone, Debug)]
pub struct GateLens;

impl Lens<System, GateState> for GateLens {
    fn get(outer: &System) -> &GateState {
        &outer.gate
    }
    fn get_mut(outer: &mut System) -> &mut GateState {
        &mut outer.gate
    }
}

/// Zoom the session machine onto one field of `System`.
pub type SessionZoom = Zoom<SessionLens, SessionMachine, System>;
/// Zoom the gate machine onto the other field of `System`.
pub type GateZoom = Zoom<GateLens, GateMachine, System>;

/// One authored routing table drives the adapter and the composed preview
/// cross edges. It is metadata about the authored router, not a row-derived
/// reachability claim.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Route {
    pub outcome: &'static str,
    pub gate_event: &'static str,
    pub gate_target: &'static str,
}

/// Authored routes from a session outcome to a gate event.
pub const ROUTES: &[Route] = &[
    Route {
        outcome: "Handled",
        gate_event: "Arm",
        gate_target: "Dormant",
    },
    Route {
        outcome: "Unhandled",
        gate_event: "Nop",
        gate_target: "Dormant",
    },
];

/// Adapter slice: session outcome selects the gate event, then delegates to the
/// zoomed gate machine. Ordinary `Then` composition; no scheduler.
pub struct GateAdapter;

impl Slice for GateAdapter {
    type Context<'a> = Cx;
    type State = System;
    type Event = SessionOutcome;
    type Output = GateOutcome;
    type Effect = Signal;

    fn reduce(
        st: &mut Self::State,
        ev: Self::Event,
        cx: Self::Context<'_>,
        fx: &mut impl FnMut(Self::Effect),
    ) -> Self::Output {
        let gate_event = match ev {
            SessionOutcome::Handled => GateEvent::Arm,
            SessionOutcome::Unhandled => GateEvent::Nop,
        };
        GateZoom::reduce(st, gate_event, cx, fx)
    }
}

/// Session dispatch feeds the authored router, which feeds the gate.
pub type RoutedSystem = Then<SessionZoom, GateAdapter>;

// ---------------------------------------------------------------------------
// Broadcast: clone a non-Copy event to the first recipient, move to the last.
// ---------------------------------------------------------------------------

/// Non-Copy event carrying owned `String` payloads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bulletin {
    pub topic: String,
    pub body: String,
}

/// Shared recipient log.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ledger {
    pub log: Vec<String>,
}

/// First recipient in declared order.
pub struct FirstRecipient;

impl Slice for FirstRecipient {
    type Context<'a> = ();
    type State = Ledger;
    type Event = Bulletin;
    type Output = ();
    type Effect = Never;

    fn reduce(
        st: &mut Self::State,
        ev: Self::Event,
        _cx: Self::Context<'_>,
        _fx: &mut impl FnMut(Self::Effect),
    ) -> Self::Output {
        st.log.push(format!("first:{}:{}", ev.topic, ev.body));
    }
}

/// Last recipient in declared order.
pub struct LastRecipient;

impl Slice for LastRecipient {
    type Context<'a> = ();
    type State = Ledger;
    type Event = Bulletin;
    type Output = ();
    type Effect = Never;

    fn reduce(
        st: &mut Self::State,
        ev: Self::Event,
        _cx: Self::Context<'_>,
        _fx: &mut impl FnMut(Self::Effect),
    ) -> Self::Output {
        st.log.push(format!("last:{}:{}", ev.topic, ev.body));
    }
}

/// Ordered two-recipient fan-out over ordinary slices. The event is cloned to
/// `A` (first) and moved to `B` (last); both see the same state. No global
/// listener registry or runtime is introduced.
pub struct Broadcast<A, B>(PhantomData<(A, B)>);

impl<A, B> Slice for Broadcast<A, B>
where
    A: Slice,
    A::Event: Clone,
    B: for<'a> Slice<
            State = A::State,
            Context<'a> = A::Context<'a>,
            Event = A::Event,
            Effect = A::Effect,
        >,
{
    type Context<'a> = A::Context<'a>;
    type State = A::State;
    type Event = A::Event;
    type Output = (A::Output, B::Output);
    type Effect = A::Effect;

    fn reduce(
        st: &mut Self::State,
        ev: Self::Event,
        cx: Self::Context<'_>,
        fx: &mut impl FnMut(Self::Effect),
    ) -> Self::Output {
        let first = A::reduce(st, ev.clone(), cx, fx);
        let last = B::reduce(st, ev, cx, fx);
        (first, last)
    }
}

/// Clone-to-first, move-to-last broadcast of a `Bulletin`.
pub type OrderedBroadcast = Broadcast<FirstRecipient, LastRecipient>;

// ---------------------------------------------------------------------------
// Pure drivers shared by the example and the tests.
// ---------------------------------------------------------------------------

/// Dispatch one event through the generated session machine.
#[allow(dead_code)]
pub fn step_session(
    st: &mut SessionState,
    ev: SessionEvent,
    cx: Cx,
    fx: &mut Vec<Signal>,
) -> SessionOutcome {
    SessionMachine::reduce(st, ev, cx, &mut |signal| fx.push(signal))
}

/// Run a session tape, collecting every outcome and effect in order.
pub fn run_session(
    tape: &[SessionEvent],
    cx: Cx,
) -> (SessionState, Vec<SessionOutcome>, Vec<Signal>) {
    let mut st = SessionState::default();
    let mut outs = Vec::new();
    let mut fx = Vec::new();
    for ev in tape {
        outs.push(SessionMachine::reduce(
            &mut st,
            ev.clone(),
            cx,
            &mut |signal| fx.push(signal),
        ));
    }
    (st, outs, fx)
}

/// Dispatch one session event through the composed `Then` routing.
pub fn step_routed(
    system: &mut System,
    ev: SessionEvent,
    cx: Cx,
    fx: &mut Vec<Signal>,
) -> GateOutcome {
    RoutedSystem::reduce(system, ev, cx, &mut |signal| fx.push(signal))
}

/// Dispatch one bulletin through the ordered broadcast.
pub fn step_broadcast(ledger: &mut Ledger, bulletin: Bulletin) {
    let _ = OrderedBroadcast::reduce(ledger, bulletin, (), &mut |never| match never {});
}
