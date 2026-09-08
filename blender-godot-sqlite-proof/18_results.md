# Executed lab results, 2026-09-07

## Core

`cargo test --locked --manifest-path core-labs/Cargo.toml`: 8 passed.

- Parry capsule intersection trace with separated/contact/separated phases.
- Rapier PhysicsWorld actual solver stepping with serialized-state restoration
  and exact replay comparison on this machine.
- Snapshot storage independence.
- GGRS local SyncTest replay and deliberately corrupted-load mismatch detection.
- Bounded ordinary SQLite frame storage.
- Prepopulated SQLite rowid slots, zero declared indexes, constant row count,
  and transaction rollback.
- Read-only virtual-table cursor generation pinning and bounded Rust slot
  row-buffer pointer/capacity reuse after release.

These are component tests, not a complete networked fighter. Delayed two-peer
network tests, whole-system allocation counts, cross-architecture determinism,
and a combined animation/physics/SQL/GGRS world remain untested.

The Ozz fixture test is implemented behind `--features ozz`. It could not run:
ozz-animation-rs 0.11.0 calls portable SIMD Mask::to_int at four source sites,
which fails compilation on the installed current and 2026-05-18 nightlies.
The dependency was not patched. Fixtures and source hashes are in fixtures/ozz.

## Presentation

`bash 10_record_collision.sh`: core tests, trace generation, Godot capture.
`bash 14_record_gpu.sh`: the same trace through a separate wgpu/Metal host.

Trace has 180 samples at 60 FPS, including 54 contact samples. Its SHA256 is
`477a9d5701fd3a490bb5382320a2f0a6d2da8845d8f9ac7e225256fd9515a49c`.
Both consumers use that file. Godot output includes one additional capture frame.
The wgpu host checks raw GPU pixels for target and the appropriate moving-capsule
color on every frame. Selected decoded frames were visually inspected.

- `12_collision.mp4`: Godot, 800x450.
- `15_gpu_collision.mp4`: wgpu/Metal, 640x480, 3 seconds.
- `6_import-proof.mp4`: Blender skinned-block import/pose-seeking proof.

This demonstrates two presentation consumers of one recorded Rust simulation
trace. It does not test live cross-host input or reproduce a game engine's full
rendering/animation stack in wgpu. GPU code is a minimal XY capsule fixture host.

## Wireframe fighter

`bash wireframe-fighter/5_run.sh` builds an editable 16-bone Blender humanoid,
exports glTF, checks animated knee pose and restoration in Godot, and captures
`wireframe-fighter/7_wire_fighter.mp4`. The fighter currently uses Godot animation,
independently of the Rust collision trace. See its 9_readme.md for the boundary.

`bash 17_run_labs.sh` reruns the original asset and collision presentation labs.
The lab was moved into `hafley-rs/games/blender-godot-sqlite-proof` on 2026-09-07.
Its two Rust packages retain isolated workspaces and lockfiles. No commit was made.

## Native PM Falcon fixture

`bash falcon-lab/3_run.sh` runs the additional isolated Rust package and records
`falcon-lab/5_falcon_knee.mp4` without Godot or Bevy. Published brawllib_rs 0.29.0
decodes public PM idle, jump, and forward-air payloads. Parry detects the attack
sphere intersecting a hovering cuboid on knee frame 14 (tick 91), adding 18 damage
once despite 15 contact ticks. Tests cover deterministic replay and a miss control.

The capture is 540 frames at 960x540/60 FPS, nine seconds including half-speed
replay. It uses extracted hurtbox bone matrices and attack data with scripted
root travel. Full skinned mesh, exact PM movement, and networked gameplay remain
outside this fixture. See `falcon-lab/7_readme.md` for precise boundaries.

## Input-driven Falcon and GGRS peers

`bash falcon-lab/9_run_rollback.sh` adds a separate increment without replacing
the earlier recording. Jump/attack input edges drive actions. A deterministic
in-memory transport holds A-to-B messages across ticks 78–96. On tick 97 GGRS
restores tick 78, replays 19 ticks, and advances once. Full peer states match from
that point onward, each with one hit and 18 damage. Zero-delay and repeat-run
controls pass. This exercises two P2P sessions without an external network.

`falcon-lab/12_rollback.mp4` displays both peers with input, action, pose, damage,
confirmation, and rollback diagnostics. See `falcon-lab/14_rollback_readme.md`.

## Sandbag launch and landing

`bash falcon-lab/15_run_launch.sh` uses ssbm_utils 0.4.0 for knockback, launch
velocity, and hitstun, then Rapier 0.35.3 for flight and floor contact. The extracted
PM knee produces scalar knockback 67.2 and 26 hitstun ticks for the fixture's
100-weight target at zero initial percent. Hit occurs on tick 91, timer expiration
on 117, and landing on 126. Full physics snapshots converge after GGRS correction
on tick 97; final damage remains 18 with one hit and zero final velocity.

`falcon-lab/18_launch.mp4` adds target phase, timer, position, velocity, and grounded
labels. This is a mixed PM-data/Melee-calculation/Rapier fixture, with approximate
movement rules. See `falcon-lab/20_launch_readme.md` for parameters and boundaries.

## Recycled SQLite presentation

`bash falcon-lab/22_run_sql.sh` tests and records the delayed peer with wgpu
consuming numeric pose, capsule, attack, and target rows through a read-only SQLite
virtual table. Three preallocated generations hold a 32-tick window. The tick-97
rollback publishes all 20 corrected frames together. A held tick-91 SQL cursor
retains damage 0 while fresh queries read damage 18; releasing it permits reuse
of the same allocation. Oversized and fully pinned publications are rejected.

Eight Falcon tests and eight core tests pass. Storage bounds apply to the
publication buffers; this fixture still allocates simulation snapshots and
rendering data. `falcon-lab/23_sql_boundary.mp4` labels publication generations,
reader state, action/pose, damage, and rollback. See `falcon-lab/26_sql_readme.md`
for schema, replay assertions, cursor-isolation scope, and reproduction commands.

## Godot consumption through gdext

`bash falcon-lab/29_run_godot.sh` builds `godot` 0.4.5 against the 4.5 API and
loads it into installed Godot 4.7. The existing Rust/GGRS/SQLite fixture emits
copied rows and shared world-space line geometry through a capacity-one bridge.
Godot renders an ArrayMesh and returns its uploaded vertices for exact comparison.
All 180 tick/generation diagnostics match the saved wgpu trace. The existing
held-reader, correction, and slot-reuse assertions execute inside the extension.

`falcon-lab/31_godot_sql.mp4` records the Godot presentation; the 180 simulation
ticks retain the earlier 824-frame slow-playback/hold schedule. This remains an
offline simulation fixture with presentation-time SQL publication. See
`falcon-lab/33_godot_readme.md` for thread ownership, capture details, and scope.

## Incremental renderer-free simulation

`simulation-core` owns the fighter and complete Rapier state without Godot,
wgpu, decoder, or SQLite dependencies. Godot requests each incremental tick;
both peers match all 180 earlier full-state observations. Core save/load at
ticks 105 and 128 reproduces every subsequent state. `falcon-lab/41_incremental.mp4`
and `falcon-lab/43_incremental_readme.md` contain the capture and boundaries.

## Independent worker and stalled consumption

`bash falcon-lab/46_run_schedule.sh` drives fixed simulation steps from a Rust
worker clock while Godot polls presentation. Consumption pauses across ticks
92 through 105. Three held SQLite readers exhaust the publication ring, causing
12 refused publications while simulation and tick-97 rollback continue. Tick 105
publishes the corrected window; consumption can resume at tick 106 with zero lag.
The tick-91 cursor still reads damage 0 while fresh queries read 18.

Uninterrupted and paused CPU runs compare both peers against all 360 typed
recorded states. The capture worker repeats those checks. Successful SQL
publications compare the entire retained window, and consumed rows and uploaded
mesh vertices round-trip exactly. Slot pointers and capacities remain unchanged.
Thirteen Falcon tests pass, and all-target gdext Clippy passes with warnings denied.

`falcon-lab/49_schedule.mp4` displays simulation/display lag, publication refusal,
rollback, damage, and reader generations. The injected pause stops adapter
consumption while the diagnostic UI continues rendering. Whole-engine/GPU stalls,
deadline guarantees, and long-running allocation bounds remain untested. See
`falcon-lab/52_schedule_readme.md` for ownership, policy, and reproduction.

## Main-thread, render-thread, and process suspension

`bash falcon-lab/55_run_faults.sh` adds bounded 800 ms delays on the Godot main
thread and a distinct rendering thread. An external Python standard-library
controller also SIGSTOPs its own Godot child, verifies OS state T, and SIGCONTs
it after 800 ms. Worker tick starts are timestamped and checked externally.

The first executed run advanced 20 worker ticks during each thread delay and
zero during the confirmed process-stopped interval. After resume, the absolute
deadline scheduler catches up by executing overdue fixed steps. All 180 ticks
execute, all 360 peer states match the existing golden fixture, and SQL windows
and uploaded meshes remain exact. The Rust worker shares Godot's process and
therefore shares whole-process suspension.

`falcon-lab/60_faults.mp4` records the resulting frame jumps and recovery;
`falcon-lab/58_faults.json` carries wall-clock evidence that MovieMaker's
frame-based timing cannot show directly. The render fault delays a CPU-side
render callback. GPU saturation, driver hangs, device loss, and driver reset
remain untested. See `falcon-lab/63_fault_readme.md` for reproduction and scope.
