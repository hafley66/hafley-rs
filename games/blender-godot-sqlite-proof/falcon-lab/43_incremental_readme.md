# Incremental simulation boundary

## Source boundary

`../simulation-core` is an isolated `falcon-simulation` Rust library. It owns the
simulation state, frozen attack/frame data, Parry contact query, and complete
Rapier sandbag state. Its normal dependency tree contains no Godot, wgpu, winit,
brawllib_rs, or rusqlite package. PM decoding, skeletal presentation, SQLite,
GGRS transport orchestration, and recording remain in the lab package.

The existing sandbag implementation moved from `1a_sandbag.rs` to
`../simulation-core/1_sandbag.rs`. The rollback fixture moved from the binary
source into `35_runtime.rs`; `8_rollback.rs` now calls the library entry point.
`36_library.rs` exports the simulation API and incremental two-peer runtime.
No library imports `8_rollback.rs` as a module.

## API and lifetime

```rust
Simulation::new(actions: Arc<[Action]>, launch: bool) -> Simulation
Simulation::advance(&mut self, input: u8) -> &World
// Execute one fixed 1/60-second step; return the current authoritative state.
Simulation::save(&self) -> Snapshot
// Copy all durable state, including Rapier contacts and solver state.
Simulation::load(&mut self, snapshot: &Snapshot)
// Restore state; subsequent inputs execute from the saved frame.

Runtime::new(actions: &[HighLevelSubaction], held: bool, launch: bool) -> Result<Runtime>
Runtime::advance(&mut self, input: u8) -> Result<[Display; 2]>
// Submit one input to the two-peer GGRS fixture and execute its requests.
// A correction may replay prior frames before advancing the current tick.
```

Signatures above omit error/lifetime parameters for readability. Simulation owns
its World and shares immutable combat data through Arc. Snapshots own durable
state copies; callers must restore with the same combat data. The runtime owns
its GGRS sessions, world states, and transport queue, and borrows the decoded
presentation assets for its lifetime. Combat data is converted once at startup.

## Execution and storage sequence

Godot's incremental mode sends input bits through a capacity-one channel. The
Rust worker waits for that request before calling `Runtime::advance`. It then
publishes all rows from the advance/replay batch atomically, queries the current
SQL generation, and returns copied rows plus shared wire geometry. Godot uploads
the mesh, acknowledges the exact rows/vertices, and holds that presentation for
the requested display frames. No future simulation trace is computed at startup.

The SQL verifier retains 32 ticks of on-time presentation rows to check corrected
history. The ring still owns three 1,024-row slots. GGRS manages its rollback
history. The recording retains its 180-entry audit report; this increment makes
no whole-program allocation or sustained-memory guarantee.

The real tick-91 SQL cursor remains open through correction at tick 97 and is
released at tick 101. It continues reading the predicted generation while fresh
queries see corrected damage. The captions add executed tick count, submitted
input, advance/replay count, and actual GGRS save/load requests.

## Verification and recording

```sh
bash 40_run_incremental.sh
```

The CPU tests compare all 180 full states of both incremental peers against
the pre-existing `17_launch_trace.json`. Recorded states are decoded into their
typed representation before comparison, preserving f32 and large-integer types.
Core save/load tests restore at checkpoints 105 and 128 and compare every
remaining state against that same recording. A separate input test branches from
one saved state and verifies that the next call's input selects idle or jump.

Outputs: `38_incremental_consumed.json`, `39_incremental_sql.json`,
`41_incremental.mp4`, and `42_incremental_frames.png`. Movie Maker's
`44_incremental.avi` intermediate is ignored. Earlier MP4s remain unchanged.
The display schedule stays at 824 frames for 180 simulation ticks, with slowed
playback and event holds. This mode uses scripted inputs submitted during
execution; keyboard input and an independently clocked simulation remain untested.

This establishes incremental execution and a renderer-free simulation package.
Separate rendering/simulation schedules, renderer-stall policy, long-running
resource bounds, network transport, cross-platform determinism, and full PM
rules remain outside this increment.
