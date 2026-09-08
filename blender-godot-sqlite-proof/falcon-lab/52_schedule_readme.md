# Independent worker and paused presentation consumer

## Boundary and existing components

`45_schedule.rs` uses `std::thread::spawn`, `Instant`, and `thread::sleep` to
drive the existing `Runtime::advance(u8)` independently of Godot. Each advance
still represents 1/60 simulation second. The capture paces these steps at 25 Hz
for visibility. There is no renderer request channel in this mode.

The existing `core_labs::sql::FrameRing<Row>::publish_rows` returns `None` when
all slots are pinned. `Boundary::publish` preserves the last generation on that
refusal. `rusqlite` 0.40.2 `Statement::query` / `Rows::next` exercise real SQLite
virtual-table cursors; each cursor pins an immutable generation through the
existing Arc-backed table implementation. The dependency remains MIT licensed,
with bundled SQLite. The lab ring is local source in `core-labs/src/3_sql.rs`.
No new dependency or scheduler framework is introduced.

## Signatures and ownership

```rust
run(stalled: bool, shared: &State, clock: impl FnMut(usize)) -> Result<Vec<Status>, Error>
// Load fixture, create runtime and SQL boundary, pace and advance 180 ticks.
// Validate both peer worlds against the previous full-state recording.
// Keep a corrected 32-tick presentation window even when publication fails.

spawn(shared: State) -> JoinHandle<Result<Vec<Status>, String>>
// Run the same harness on a worker with an Instant-based clock.

FalconSql::poll_scheduled(consume: bool) -> VarDictionary
// Observe current diagnostics. Outside the injected pause, query the latest
// published generation if it differs from the last acknowledged generation.
// Return copied rows and mesh data for Godot upload and exact readback checks.
```

The worker owns Runtime, its SQL boundary, corrected pending window, and three
fixture reader connections. Their Statements and Rows stay in the worker's
stack scope. Godot owns its mesh and last displayed frame. Shared metadata owns
a ring handle, current status, and latest successful publication status. A
short mutex section keeps publication and metadata coherent with consumer SQL
reads. Mesh upload and capture pacing occur outside that section. This does not
establish a lock-free or hard-real-time execution guarantee.

## Executed schedule and overflow policy

| Simulation tick | Operation |
| --- | --- |
| 90, 91, 92 | Hold three SQLite cursors, pinning all publication slots. |
| 92 through 105 | Adapter stops consuming frames; Godot retains its mesh and updates diagnostic labels. |
| 93 through 104 | Publication refuses; simulation continues and replaces replayed ticks in its pending window. |
| 97 | Delayed input restores tick 78 and executes 20 advances, including replay. |
| 105 | Release two cursors; publish the latest corrected window. |
| 106 | Consumer can resume directly from the latest generation. |
| 110 | Drain and verify the tick-91 cursor, then release it. |
| 179 | Verify final consumed generation and finish the recording. |

Overflow policy: skip publication, preserve the last readable generation, retain
only the latest corrected 32-tick presentation window, and retry after the next
simulation step. Skipped presentation generations are never enqueued for later
display. An authoritative step is never skipped because publication refuses.
This fixture tests 12 refused publications and correction during that interval.
Exhaustion lasting beyond the history window remains untested.

The held tick-91 cursor retains damage 0. After publication recovers, a fresh
tick-91 query reads damage 18. All slot pointers and capacities remain unchanged.
Every successful publication compares all SQL rows with the entire pending
corrected window, including corrections accumulated while publication refused.
The record has separate simulation tick, publication tick, and display tick;
generation identifiers no longer have to equal simulation tick plus one.

## Reproduction and scope

Run `bash 46_run_schedule.sh`. It runs deterministic CPU tests, builds gdext,
checks extension loading, records Godot, encodes H.264 `49_schedule.mp4`, and
extracts `50_schedule_frames.png`. Raw `51_schedule.avi` is ignored. Earlier
recordings and their evidence remain unchanged.

The runner checks GDScript parsing before capture, limits failed captures to
1,800 frames, and requires success markers in `53_schedule_run.log` before
encoding. `RenderingServer.force_draw(false)` ensures the recorded surface is
updated: inspection of the first capture found stale final pixels despite
successful mesh acknowledgements. Explicit drawing produced the final tick-179
image as well as the paused and resumed states.

`47_worker_schedule.json` records all 180 producer observations.
`48_schedule_consumed.json` records actual Godot SQL reads and row/mesh readback
checks. The CPU test runs uninterrupted and paused schedules against all 360
typed peer states in `17_launch_trace.json`, including Rapier state. Capture
repeats those full-state assertions in the worker. Consumer observations and
video frame count may vary with wall-clock scheduling; simulation results and
the injected tick-indexed schedule are deterministic.

The pause is injected at adapter consumption. Godot's diagnostic UI continues
rendering throughout. Whole-process suspension, GPU driver stalls, arbitrary
pause schedules, deadline guarantees, cross-platform determinism, keyboard input,
network transport, and sustained allocation bounds remain untested.

## Recorded result

The final run passed 13 Falcon tests and all-target gdext Clippy with warnings
denied. The dependency `block` 0.1.6 emits Cargo's separate future-incompatibility
notice. The worker verified 360 full peer states and 12 refused publications.
Godot acknowledged 165 observations, jumping from tick 91 to tick 106 across the
pause and finishing at tick 179, generation 168. All consumed rows and uploaded
vertices matched. The MP4 is H.264, 960x540, 476 frames, 7.933333 seconds, and
540,808 bytes. Inspected montage and final frame show the pause, corrected
resumption, and final tick 179. Godot's console movie frame count differed from
the encoded artifact; these video measurements are from ffprobe.
