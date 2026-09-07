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
