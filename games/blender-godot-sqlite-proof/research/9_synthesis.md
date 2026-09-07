# Foundation synthesis

Research only. No build, implementation, experiment, recording, or allocation
measurement was executed.

## Concrete library combination

- **Simulation collision/controller/dynamics:** Rapier 0.35.3 pinned to release
  commit `b82079a`, with `serde-serialize` and `enhanced-determinism`, excluding
  `simd8`. Use its kinematic character controller plus the required rigid-body
  or collision pipeline. Determinism conditions and snapshot exclusions are in
  [Rapier's guides](https://rapier.rs/docs/user_guides/templates_injected/determinism/).
- **Skeletal runtime:** ozz-animation-rs 0.11.0 pinned to
  `ece37e90f2df7ba2747f4886f420a377897bb52c`; use fixed integer animation phase,
  reusable sampling/model buffers, and the matching offline ozz converter.
- **Rollback:** GGRS v0.13.0 after resolving and recording its full tag SHA;
  implement every ordered save/load/advance request and checksum canonical state.
- **SQL:** rusqlite 0.40.2 with `vtab`, plus one pinned bundled SQLite version.
  Use a read-only virtual table for inspection of immutable retained ring
  generations. Use ordinary indexed tables in transactions for durable confirmed
  frames, replays, analytics, and joins.
- **Hosts:** godot-rust `godot` 0.5.5 against a pinned Godot 4.x API/runtime and
  winit 0.30.13 plus wgpu 30.0.1 as the second presentation host.

Tnua and Bevy are excluded. The same Rust `Simulation` instance shape, fixed
tick, normalized inputs, rollback adapter, collision state, and animation state
are used by both hosts.

## Boundary and lifetime sketch

```rust
fn advance(state: &mut SimState, input: FrameInputs, scratch: &mut Scratch)
    -> StepEvents;
fn save(state: &SimState, out: &mut SnapshotSlot) -> SnapshotMeta;
fn load(state: &mut SimState, snapshot: &SnapshotSlot);
fn present<'a>(state: &'a SimState, out: &'a mut PresentBuffers)
    -> PresentationFrame<'a>;
fn publish_sql(ring: &mut FrameRing<N>, frame: FrameId, view: SimSqlView<'_>);
```

`SimState` owns stable-ID dense gameplay arrays, Rapier durable structures,
integer tick/animation phase, blend/root-motion/IK/event state, RNG state, and
confirmed-side-effect cursor. `Scratch` owns reusable Rapier/Parry and ozz work
buffers and is rebuilt after load only where equality tests permit. Snapshot
slots own independent bytes/values for at least the GGRS retention window.
Presentation hosts borrow immutable canonical output until `present` returns.
SQL cursors hold an immutable ring generation; the writer cannot recycle that
slot while a guard exists.

Sequence: normalize host input, give it to GGRS, execute ordered save/load/step
requests, compute canonical checksum, publish confirmed presentation/SQL views,
then let each host convert/upload. Host time, node/GPU handles, SQL connection,
and speculative audiovisual effects never enter the simulation checksum.

## Unresolved test gates

1. Resolve one lockfile and record full SHAs, MSRVs, licenses, bundled SQLite,
   target features, and Godot API/precision pairing.
2. Prove Rapier save/load including controller/app callback state and its stated
   construction-order/compiler/architecture constraints.
3. Prove ozz fresh-versus-restored sampling context equality, converter pairing,
   matrix/handedness/inverse-bind conventions, and x86_64/ARM64 results.
4. Run GGRS sync-test and adversarial two-peer replay, including corruption,
   delayed/out-of-order inputs, snapshot aliasing, and side-effect deduplication.
5. Audit rusqlite vtab result lifetimes/copies at 0.40.2; prove cursor generation
   pinning, query plans, transaction visibility, and ordinary-table persistence.
6. Measure allocations after warmup in every stage. Wgpu documents staging
   allocation for `Queue::write_buffer`; Godot bulk upload behavior is `[UK]`.
7. Run identical input artifacts through Godot and winit/wgpu at different render
   cadences, compare core hashes and pre-conversion presentation bytes, and record
   verified H.264 output from the actual hosts.

These gates are proposed. Results must remain unknown until executed. Detailed
evidence and adversarial cases are in [1_collision.md](1_collision.md),
[2_animation.md](2_animation.md), [3_rollback.md](3_rollback.md),
[4_sql_buffer.md](4_sql_buffer.md), [5_hosts.md](5_hosts.md),
[6_compatibility.md](6_compatibility.md), [7_replay_audit.md](7_replay_audit.md),
and [8_storage_audit.md](8_storage_audit.md).
