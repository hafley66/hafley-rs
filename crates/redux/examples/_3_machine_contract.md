# Shared machine contract audit

Status: source audit and implementation contract. This file records demonstrated
behavior and a bounded ordinary-Rust proof shape. It does not claim an engine
integration or a statig replacement.

## Existing seams and exact upstream behavior

- `redux::Slice` is `reduce(&mut State, Event, Context, &mut FnMut(Effect)) -> Output`
  (`crates/redux/src/0_slice.rs:9-23`). `Context<'a>: Copy`; state is caller-owned.
- `Then` passes A's output to B while sharing state, context, and effect sink
  (`0_slice.rs:28-56`). `Zoom` selects one nested state by `Lens::get_mut`
  (`0_slice.rs:82-110`). `Each` owns `[Inner::State; N]`, asks its provider for
  each slot's event/context, and reduces slots `0..live` (`0_slice.rs:202-237`).
- `_2_hierarchical_slice.rs` demonstrates statig blocking hierarchy, lifecycle
  descriptors, clone suffix equality, serde re-entry, and two independent
  machines (`_2_hierarchical_slice.rs:47-67,144-233,415-495`).
- statig 0.4.1 `Inner::transition` calls `before_transition`, computes
  `transition_path`, exits, swaps state, enters, then calls `after_transition`
  (`blocking/inner.rs:40-59`). `StateExt::transition_path` returns `(1, 1)` for
  same-state transitions and otherwise uses common-ancestor depth
  (`blocking/state.rs:39-84`). Child handlers return `Super` to climb to the
  parent; entry and exit actions run on the calculated path
  (`blocking/state.rs:86-168`).
- `StateMachine::handle_with_context` initializes lazily when `initialized` is
  false (`blocking/state_machine.rs:67-93`). `Clone` copies `Inner` and the
  `initialized` flag (`blocking/state_machine.rs:206-216`).
- statig serde serializes `Inner` only, whose fields are `shared_storage` and
  `state` (`blocking/state_machine.rs:286-299`, `blocking/inner.rs:92-108`).
  Deserializing `StateMachine` restores those fields and sets
  `initialized: false` (`blocking/state_machine.rs:301-320`). The documented
  `UninitializedStateMachine::init_with_context` then executes the initial
  entry path (`blocking/state_machine.rs:566-648`).

Restore consequence: a clone of an initialized machine resumes with the same
state, storage, hook history, and effect suffix. A serde restore emits nothing
during decoding, then the next dispatch re-runs every entry action on the
restored path. Mutable entry writes repeat, and effect descriptors emitted by
those actions repeat. Exit actions do not run during restore. The reference
test records this at `_2_hierarchical_slice.rs:437-478` and
`crates/redux/tests/2_statechart.rs:233-309`. A transparent serde restore
requires an upstream statig representation that serializes and restores the
initialized bit, or an equivalent initialized constructor that suppresses
initial entry actions. The authored restriction alternative is explicit:
serde checkpoints may contain only charts whose initialization and entry hooks
are inert; ordinary event handlers may mutate storage and emit effects.
Safe restore paths: clone the complete initialized `StateMachine`; use serde under
the inert-hook restriction; or obtain an upstream initialized deserialize/from-parts API.
Internal flag edits, unsafe state mutation to mark initialization, and hook effect filtering fail.

Instance lifetime: construct one `MachineId`, initialize its chart once, dispatch ticks, clone at a checkpoint, replace on rollback, and publish after catch-up.
## Reusable APIs and boundaries

`game_input::PlayerInput` is four signed axes plus a button bitset
(`crates/input/src/_0_types.rs:3-10`). `History::advance` stores the prior
sample and derives pressed/released masks (`_3_history.rs:5-35`). The composed
`Intent<A, Aim>` owns its pending payload and statig buffer, with replacement,
expiry, cancellation, and serde state (`_2_buffer.rs:42-198`).

`game_physics::PhysicsSlice` owns a `rapier3d::PhysicsWorld` and accepts only
`Step`; it emits `Never` (`crates/physics/src/_1_slice.rs:4-35`).
`clone_world` rebuilds the two skipped Rapier workspaces while cloning durable
fields (`_0_world.rs:3-28`). There is currently no reusable physics command or
stable machine-to-body target API. A proof may therefore use an inert command
descriptor and apply it in a later adapter.

The fighter chart currently rebuilds a stack-local statig machine from a phase
on every `decide` call and reports only an optional transition
(`crates/fighter/src/_1a_chart.rs:142-156`). Numeric velocity, gravity, jump
budget, contact, and landing writes remain in the owning fighter state
(`crates/fighter/src/_2_advance.rs:315-370,465-519`).

## Alternative: statig generic extension shape

This alternative awaits restore resolution and does not control the existing GLM
plain-data `Then`/`Zoom` proof. `Chart<E>` is statig storage plus one `Phase`
state; the outer value owns all other mutable causes of future output.

```rust
pub struct MachineId(pub u32);
pub struct BodyId(pub u32);
pub struct PhysicsTarget { pub machine: MachineId, pub body: BodyId }
pub struct TickCx<'a> { pub rules: &'a Rules, pub target: PhysicsTarget }
pub enum Event { Input(Frame), Contact(Contact), Tick }
pub enum PhysicsCommand { SetVelocity { target: BodyId, value: [f32; 2] } }
pub enum Effect { Physics(PhysicsCommand), Lifecycle(Lifecycle) }
pub struct Locomotion<E> {
    pub chart: StateMachine<Chart<E>>, // sole Phase owner
    pub history: History,
    pub phase_clock: u32,
    pub jumps_left: u8,
}
pub struct LocomotionSlice<E>(PhantomData<E>);
impl<E: AirJumpExtension> Slice for LocomotionSlice<E> {
    type Context<'a> = TickCx<'a>; type State = Locomotion<E>;
    type Event = Event; type Output = Phase; type Effect = Effect;
    fn reduce(st: &mut Self::State, ev: Self::Event, cx: Self::Context<'_>,
              fx: &mut impl FnMut(Self::Effect)) -> Self::Output;
}
```

The body sequence is: derive input edges before dispatch; let the extension
handle its leaf rule; on `Fallback` return `Super` into shared grounded or
airborne rules; apply one transition through the chart; emit targeted inert
physics commands; advance `phase_clock` according to the transition kind; return
the chart phase. A child `Handled` result ends dispatch. A child
`Transition(next)` uses the same phase owner. The shared chart supplies the
grounded, airborne, landing, and common air-jump rules once. Shooter and fighter
extensions supply only their policy-specific air-jump cases through
`AirJumpExtension::handle`, with `Handled`, `Transition(Phase)`, or `Fallback`.

Handler work may mutate storage or emit commands with any handler outcome.
Transition/lifecycle work is separate: a phase transition computes its exit/entry
path; `Handled` has no transition path. A declared
self-transition is significant when it must run leaf lifecycle or reset the
phase clock; the contract carries that choice as `TransitionKind::Self` rather
than inferring it from equal enum values. `Phase` is stored once inside
`StateMachine`; `phase_clock`, input history, jump budget, and command-relevant
physics facts are snapshot state.

`PhysicsCommand` targets `BodyId`; `PhysicsTarget` carries the explicit
`MachineId -> BodyId` relation as proof data, with no adapter implementation.
`MachineId` addresses durable machine state; Rapier body handles address physics
state. `Grounded`, `Airborne`, and leaf phases are internal chart states or
superstates and do not identify machine instances.

## Acceptance matrix

| Case | Required assertion |
| --- | --- |
| Same input/state | Original and clone receive the same suffix and return equal phase, durable state, and effect sequence. |
| Clone restore | Clone construction emits no effects; suffix includes mutable storage and lifecycle effects exactly once. |
| Serde suffix gate | Serialize/deserialize, run the same suffix, and require equal state and effect sequence; hooked charts currently fail this gate. |
| Hooked serde evidence | Negative test records repeated mutable/emitted entry hooks as blocker evidence; it cannot substitute for the serde gate. |
| Two addressed instances | `Each<LocomotionSlice<_>, _, 2>` or an equivalent fixed pair routes events and `MachineId -> BodyId` targets; advancing one leaves the other unchanged. |
| Rule ownership | Common grounded/airborne transition inventory has one owner; extensions add policy cases and contain no copied common transition rules. |

## Future metadata and proof boundary

Each authored edge needs `source`, `target`, `event`, guard identity, priority,
transition kind, clock write, effect kinds, and owner. Each node needs its
parent, leaf/superstate role, entry/exit hook classification, and clock owner.
Each machine declaration needs its `MachineId` field, state lens, event domain,
effect domain, and `Each` address mapping. This is the minimum input for later
duplicate-edge and clock-DAG checks. No macro or framework layer is specified.

The controlling GLM proof remains a plain-data state composed with existing
`Slice`, `Then`, and `Zoom`; use `Each` if needed. The statig generic shape awaits
restore resolution. Engine loops, networking, macro generation, transparent serde
restoration, and a physics command adapter remain outside this document.
