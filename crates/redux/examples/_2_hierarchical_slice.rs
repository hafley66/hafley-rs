//! Hierarchical statig =0.4.1 inside the existing `redux::Slice` seam.
//!
//! # The seam (what already exists)
//!
//! - `crates/redux/src/0_slice.rs:11` — `Slice::reduce(st, ev, cx, fx) -> Output`, effects
//!   escape only as inert descriptors through `fx: &mut impl FnMut(Effect)`.
//! - `crates/fighter/src/_1a_chart.rs:149` — today's statig usage hand-implements
//!   `IntoStateMachine` without the `macro` feature (the `statig` dev-dependency here enables
//!   `serde` but not `macro`), rebuilds the machine per decide call, and has no entry/exit
//!   actions, so its per-call `init_with_context` is a no-op. This lab keeps the same
//!   macro-free style but adds entry/exit actions, where a per-decide re-`init` would rerun
//!   entry effects; the machine instance is therefore the durable state itself.
//!
//! # Concrete trace of this example
//!
//! `Node::Connected` is the superstate of `Idle | Downloading | Uploading`;
//! `Disconnected` is a top-level leaf with no superstate.
//!
//! | before             | event                   | emitted effects                                | after           |
//! |--------------------|-------------------------|------------------------------------------------|-----------------|
//! | (boot)             | `boot()`                | `Enter(Connected)`, `Enter(Idle)`              | `Idle`          |
//! | `Idle`             | `StartDownload`         | `Exit(Idle)`, `Enter(Downloading)`             | `Downloading`   |
//! | `Downloading`      | `SwitchToUpload`        | `Exit(Downloading)`, `Enter(Uploading)`        | `Uploading`     |
//! | `Downloading`      | `ConnectionLost`        | `Exit(Downloading)`, `Exit(Connected)`, `Enter(Disconnected)` | `Disconnected` |
//! | `Disconnected`     | `Reconnect`             | `Exit(Disconnected)`, `Enter(Connected)`, `Enter(Idle)` | `Idle` |
//! | `Downloading`      | `Pause`                 | (none; child precedence, parent never sees it) | `Downloading`   |
//! | `Downloading`      | `Restart` (self)        | `Exit(Downloading)`, `Enter(Downloading)`      | `Downloading`   |
//! | `Downloading`      | `Suspend` (handled)     | (none)                                         | `Downloading`   |
//!
//! Sibling transitions and self-transitions compute `(exit_levels, enter_levels) = (1, 1)`
//! (`statig::blocking::StateExt::transition_path` returns `(1, 1)` when
//! `same_state(source, target)` and the common ancestor is the parent otherwise), so
//! `Connected` entry/exit actions never rerun for them.
//!
//! # Signatures
//!
//! - `pub fn boot(fx: &mut impl FnMut(Effect)) -> NetState` — builds the machine seeded at
//!   `Idle` and runs `init_with_context` exactly once; the full entry chain is emitted
//!   through the same `fx` channel the `Slice` uses. This is the only time entry effects
//!   are emitted for a freshly constructed state.
//! - `NetSlice: Slice` with `State = StateMachine<Chart>`, `Event = NetEvent`,
//!   `Output = NetPhase`, `Effect = Effect`, `Context<'a> = ()`; `reduce` calls
//!   `handle_with_context` and returns the current phase.
//! - `Effect` is an inert typed descriptor (`Enter`/`Exit` of a `Node`); nothing in this
//!   example performs I/O.
//!
//! # Durable ownership and the restore sequence
//!
//! Exactly one durable owner: `NetState = statig::blocking::StateMachine<Chart>` is the
//! `Slice::State` value. It alone holds the phase (`M::State`), the shared data
//! (`Chart::bytes_done`), and statig's `initialized` flag. There is no separate phase
//! copy and no separate data copy.
//!
//! - Construction/initialization: `boot(fx)` seeds `state_mut()` with `M::initial()`
//!   (`Idle`), then `init_with_context` runs the entry chain once and emits it.
//! - Restore (exact): `Clone` — `Inner` is cloned including the `initialized: true` flag
//!   (`state_machine.rs:206`), so the next dispatch performs no entry actions.
//! - Restore (serde): upstream serializes `{shared_storage, state}` only
//!   (`inner.rs:92`); `StateMachine::deserialize` (`state_machine.rs:305`) restores both
//!   fields but resets `initialized: false`, so the next `handle_with_context` re-runs the
//!   full entry chain through the public API. The documented-path alternative
//!   (`UninitializedStateMachine::init`, per the serde doc comments) does the same by
//!   design. Precise upstream limitation: **a serde checkpoint cannot be restored as an
//!   initialized machine without re-running entry effects**; only `Clone` is exact. The
//!   `serde_checkpoint` test below pins this exact re-entry trace instead of hiding it;
//!   rollback code must checkpoint by `Clone` (as `tests/2_statechart.rs` already does).
//!
//! # Running
//!
//! `cargo test --manifest-path crates/redux/Cargo.toml --example _2_hierarchical_slice`
//! runs the inline tests; `cargo run` on the same example prints the trace table.

use redux::Slice;
use serde::{Deserialize, Serialize};
use statig::blocking::{self, IntoStateMachine, IntoStateMachineExt, StateMachine};
use statig::{
    Outcome,
    Outcome::{Handled, Super, Transition},
};

/// One node of the hierarchy, named for effect descriptions only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Node {
    Connected,
    Idle,
    Downloading,
    Uploading,
    Disconnected,
}

/// Inert lifecycle descriptor. The runner outside rollback state decides what to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    Enter(Node),
    Exit(Node),
}

/// Network events for the lab chart.
#[derive(Clone, Copy, Debug)]
pub enum NetEvent {
    StartDownload,
    Progress { bytes: u32 },
    Suspend,
    Pause,
    Restart,
    SwitchToUpload,
    ConnectionLost,
    Reconnect,
}

/// Leaf states. `Connected` is deliberately not a leaf variant; it lives in [`NetSuper`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetPhase {
    Idle,
    Downloading,
    Uploading,
    Disconnected,
}

/// The single superstate: shared handlers for the three connected children.
pub enum NetSuper {
    Connected,
}

/// Shared storage owned by the machine: the durable data byte count.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chart {
    pub bytes_done: u64,
}

/// Borrowed sink passed through statig's external context; lifecycle effects land here.
pub struct Ex<'a> {
    pub emit: &'a mut dyn FnMut(Effect),
}

impl NetPhase {
    fn node(self) -> Node {
        match self {
            NetPhase::Idle => Node::Idle,
            NetPhase::Downloading => Node::Downloading,
            NetPhase::Uploading => Node::Uploading,
            NetPhase::Disconnected => Node::Disconnected,
        }
    }
}

impl IntoStateMachine for Chart {
    type Event<'evt> = NetEvent;
    type Context<'ctx> = Ex<'ctx>;
    type State = NetPhase;
    type Superstate<'sub> = NetSuper;

    fn initial() -> NetPhase {
        NetPhase::Idle
    }
}

impl blocking::State<Chart> for NetPhase {
    fn call_handler(&mut self, data: &mut Chart, ev: &NetEvent, _cx: &mut Ex<'_>) -> Outcome<Self> {
        use NetEvent::*;
        match (self, ev) {
            // Every leaf defers ConnectionLost to its superstate. `Disconnected`
            // has no superstate, so `Super` terminates as a handled no-op.
            (_, ConnectionLost) => Super,
            (NetPhase::Idle, StartDownload) => Transition(NetPhase::Downloading),
            (NetPhase::Downloading, Progress { bytes }) => {
                data.bytes_done += u64::from(*bytes);
                Handled
            }
            (NetPhase::Downloading, Suspend) => Handled,
            // Child precedence: the parent's own `Pause` rule below never runs
            // while a child handles the event first.
            (NetPhase::Downloading, Pause) | (NetPhase::Uploading, Pause) => Handled,
            // Significant self-transition: restarts the leaf entry chain only.
            (NetPhase::Downloading, Restart) => Transition(NetPhase::Downloading),
            (NetPhase::Downloading, SwitchToUpload) => Transition(NetPhase::Uploading),
            (NetPhase::Uploading, Progress { bytes }) => {
                data.bytes_done += u64::from(*bytes);
                Handled
            }
            (NetPhase::Disconnected, Reconnect) => Transition(NetPhase::Idle),
            _ => Super,
        }
    }

    fn call_entry_action(&mut self, _: &mut Chart, cx: &mut Ex<'_>) {
        (cx.emit)(Effect::Enter(self.node()));
    }

    fn call_exit_action(&mut self, _: &mut Chart, cx: &mut Ex<'_>) {
        (cx.emit)(Effect::Exit(self.node()));
    }

    fn superstate(&mut self) -> Option<NetSuper> {
        match self {
            NetPhase::Disconnected => None,
            _ => Some(NetSuper::Connected),
        }
    }
}

impl blocking::Superstate<Chart> for NetSuper {
    fn call_handler(&mut self, _: &mut Chart, ev: &NetEvent, _: &mut Ex<'_>) -> Outcome<NetPhase> {
        match ev {
            // ConnectionLost reached the parent through leaf `Super` returns.
            NetEvent::ConnectionLost => Transition(NetPhase::Disconnected),
            // Parent-level rule that child handlers shadow while connected.
            NetEvent::Pause => Transition(NetPhase::Idle),
            _ => Handled,
        }
    }

    fn call_entry_action(&mut self, _: &mut Chart, cx: &mut Ex<'_>) {
        (cx.emit)(Effect::Enter(Node::Connected));
    }

    fn call_exit_action(&mut self, _: &mut Chart, cx: &mut Ex<'_>) {
        (cx.emit)(Effect::Exit(Node::Connected));
    }
}

/// The durable machine value itself is the `Slice::State`; nothing else stores a phase.
pub type NetState = StateMachine<Chart>;

/// Construct and initialize the durable state. The initial entry chain
/// (`Enter(Connected)`, `Enter(Idle)`) is emitted through `fx` exactly once.
pub fn boot(fx: &mut impl FnMut(Effect)) -> NetState {
    let mut machine: NetState = Chart::default().uninitialized_state_machine().into();
    machine.init_with_context(&mut Ex { emit: fx });
    machine
}

/// One dispatch through the machine; lifecycle effects escape as descriptors only.
struct NetSlice;

impl Slice for NetSlice {
    type Context<'a> = ();
    type State = NetState;
    type Event = NetEvent;
    type Output = NetPhase;
    type Effect = Effect;

    fn reduce(
        st: &mut Self::State,
        ev: Self::Event,
        _cx: Self::Context<'_>,
        fx: &mut impl FnMut(Self::Effect),
    ) -> Self::Output {
        st.handle_with_context(&ev, &mut Ex { emit: fx });
        *st.state()
    }
}

fn run(machine: &mut NetState, ev: NetEvent) -> (NetPhase, Vec<Effect>) {
    let mut effects = Vec::new();
    let phase = NetSlice::reduce(machine, ev, (), &mut |fx| effects.push(fx));
    (phase, effects)
}

fn main() {
    let mut fx = Vec::new();
    let mut machine = boot(&mut |e| fx.push(e));
    println!("boot: phase={:?} effects={fx:?}", machine.state());
    for ev in [
        NetEvent::StartDownload,
        NetEvent::Progress { bytes: 128 },
        NetEvent::SwitchToUpload,
        NetEvent::Restart,
        NetEvent::ConnectionLost,
        NetEvent::Reconnect,
    ] {
        let (phase, effects) = run(&mut machine, ev);
        println!(
            "{ev:?}: phase={phase:?} effects={effects:?} bytes_done={}",
            machine.inner().bytes_done
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node_run(machine: &mut NetState, events: &[NetEvent]) -> (NetPhase, Vec<Effect>) {
        let mut effects = Vec::new();
        let mut phase = NetPhase::Idle;
        for ev in events {
            phase = NetSlice::reduce(machine, *ev, (), &mut |fx| effects.push(fx));
        }
        (phase, effects)
    }

    #[test]
    fn boot_emits_parent_then_child_entry_exactly_once() {
        let mut fx = Vec::new();
        let machine = boot(&mut |e| fx.push(e));
        assert_eq!(
            fx,
            vec![Effect::Enter(Node::Connected), Effect::Enter(Node::Idle)]
        );
        assert_eq!(machine.state(), &NetPhase::Idle);
    }

    #[test]
    fn sibling_transition_exits_enters_children_without_parent_entry() {
        let mut machine = boot(&mut |_| {});
        let (phase, effects) = node_run(
            &mut machine,
            &[NetEvent::StartDownload, NetEvent::SwitchToUpload],
        );
        assert_eq!(phase, NetPhase::Uploading);
        assert_eq!(
            effects,
            vec![
                Effect::Exit(Node::Idle),
                Effect::Enter(Node::Downloading),
                // No `Enter(Connected)`: the parent stays entered across the
                // sibling hop. Repeated parent entry fails this exact array.
                Effect::Exit(Node::Downloading),
                Effect::Enter(Node::Uploading),
            ]
        );
    }

    #[test]
    fn connection_lost_bubbles_exiting_child_then_parent() {
        let mut machine = boot(&mut |_| {});
        let (phase, effects) = node_run(
            &mut machine,
            &[NetEvent::StartDownload, NetEvent::ConnectionLost],
        );
        assert_eq!(phase, NetPhase::Disconnected);
        assert_eq!(
            effects,
            vec![
                Effect::Exit(Node::Idle),
                Effect::Enter(Node::Downloading),
                Effect::Exit(Node::Downloading),
                Effect::Exit(Node::Connected),
                Effect::Enter(Node::Disconnected),
            ]
        );
    }

    #[test]
    fn reconnect_enters_parent_then_child() {
        let mut machine = boot(&mut |_| {});
        let (phase, effects) = node_run(
            &mut machine,
            &[NetEvent::ConnectionLost, NetEvent::Reconnect],
        );
        assert_eq!(phase, NetPhase::Idle);
        assert_eq!(
            effects,
            vec![
                Effect::Exit(Node::Idle),
                Effect::Exit(Node::Connected),
                Effect::Enter(Node::Disconnected),
                Effect::Exit(Node::Disconnected),
                Effect::Enter(Node::Connected),
                Effect::Enter(Node::Idle),
            ]
        );
    }

    #[test]
    fn child_handler_precedes_parent_rule() {
        // In `Downloading` the leaf shadows the parent's `Pause` rule.
        let mut machine = boot(&mut |_| {});
        let (phase, effects) = node_run(&mut machine, &[NetEvent::StartDownload, NetEvent::Pause]);
        assert_eq!(phase, NetPhase::Downloading);
        assert_eq!(
            effects,
            vec![Effect::Exit(Node::Idle), Effect::Enter(Node::Downloading)]
        );
        // Without a child handler the parent's rule applies.
        let mut machine = boot(&mut |_| {});
        let (phase, effects) = node_run(&mut machine, &[NetEvent::Pause]);
        assert_eq!(phase, NetPhase::Idle);
        assert_eq!(
            effects,
            vec![Effect::Exit(Node::Idle), Effect::Enter(Node::Idle)]
        );
    }

    #[test]
    fn self_transition_reenters_leaf_only_and_keeps_data() {
        let mut machine = boot(&mut |_| {});
        let (phase, effects) = node_run(
            &mut machine,
            &[
                NetEvent::StartDownload,
                NetEvent::Progress { bytes: 64 },
                NetEvent::Restart,
            ],
        );
        assert_eq!(phase, NetPhase::Downloading);
        assert_eq!(
            effects,
            vec![
                Effect::Exit(Node::Idle),
                Effect::Enter(Node::Downloading),
                Effect::Exit(Node::Downloading),
                Effect::Enter(Node::Downloading),
            ]
        );
        assert_eq!(machine.inner().bytes_done, 64);
    }

    #[test]
    fn handled_event_is_a_no_op() {
        let mut machine = boot(&mut |_| {});
        let (phase, effects) =
            node_run(&mut machine, &[NetEvent::StartDownload, NetEvent::Suspend]);
        assert_eq!(phase, NetPhase::Downloading);
        assert_eq!(
            effects,
            vec![Effect::Exit(Node::Idle), Effect::Enter(Node::Downloading)]
        );
        assert_eq!(machine.state(), &NetPhase::Downloading);
    }

    #[test]
    fn clone_restore_replays_identical_suffix_state_and_effects() {
        let prefix = [NetEvent::StartDownload, NetEvent::Progress { bytes: 5 }];
        let suffix = [
            NetEvent::Progress { bytes: 2 },
            NetEvent::SwitchToUpload,
            NetEvent::ConnectionLost,
            NetEvent::Reconnect,
        ];
        let mut original = boot(&mut |_| {});
        node_run(&mut original, &prefix);
        let mut restored = original.clone();
        // The clone itself emits nothing: restore is a plain value copy.
        let (original_phase, original_fx) = node_run(&mut original, &suffix);
        let (restored_phase, restored_fx) = node_run(&mut restored, &suffix);
        assert_eq!(original_phase, restored_phase, "phase suffix must match");
        assert_eq!(original_fx, restored_fx, "effect suffix must match");
        assert!(original == restored, "full durable state must match");
        assert_eq!(restored.inner().bytes_done, 7);
    }

    #[test]
    fn serde_checkpoint_restore_emits_nothing_then_reenters_once() {
        let prefix = [NetEvent::StartDownload, NetEvent::Progress { bytes: 5 }];
        let mut original = boot(&mut |_| {});
        node_run(&mut original, &prefix);
        let bytes = serde_json::to_vec(&original).unwrap();
        // Deserialization itself emits nothing.
        let mut decoded: NetState = serde_json::from_slice(&bytes).unwrap();
        // Upstream limitation, pinned exactly: `StateMachine::deserialize`
        // restores `{shared_storage, state}` but clears `initialized`, so the
        // next dispatch re-runs the full entry chain for the restored leaf
        // (`Enter(Connected)`, `Enter(Downloading)`) before its own effects.
        let (decoded_phase, decoded_fx) = node_run(&mut decoded, &[NetEvent::SwitchToUpload]);
        assert_eq!(decoded_phase, NetPhase::Uploading);
        assert_eq!(
            decoded_fx,
            vec![
                Effect::Enter(Node::Connected),
                Effect::Enter(Node::Downloading),
                Effect::Exit(Node::Downloading),
                Effect::Enter(Node::Uploading),
            ]
        );
        // Contrast: the clone path is exact and re-enters nothing.
        let mut cloned = original.clone();
        let (cloned_phase, cloned_fx) = node_run(&mut cloned, &[NetEvent::SwitchToUpload]);
        assert_eq!(cloned_phase, NetPhase::Uploading);
        assert_eq!(
            cloned_fx,
            vec![
                Effect::Exit(Node::Downloading),
                Effect::Enter(Node::Uploading),
            ]
        );
        assert!(
            cloned == decoded,
            "phases and data converge; only entry replay differs"
        );
        assert_ne!(
            cloned_fx, decoded_fx,
            "raw serde restore is not an exact rollback snapshot"
        );
    }

    #[test]
    fn two_instances_remain_independent() {
        let mut a = boot(&mut |_| {});
        let mut b = boot(&mut |_| {});
        let (a_phase, _) = node_run(
            &mut a,
            &[NetEvent::StartDownload, NetEvent::Progress { bytes: 42 }],
        );
        assert_eq!(a_phase, NetPhase::Downloading);
        assert_eq!(a.inner().bytes_done, 42);
        assert_eq!(
            b.state(),
            &NetPhase::Idle,
            "untouched instance must not move"
        );
        assert_eq!(b.inner().bytes_done, 0);
        let (b_phase, _) = node_run(&mut b, &[NetEvent::ConnectionLost]);
        assert_eq!(b_phase, NetPhase::Disconnected);
        assert_eq!(
            a.state(),
            &NetPhase::Downloading,
            "a unaffected by b's event"
        );
    }
}
