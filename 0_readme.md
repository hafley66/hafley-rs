# Game development labs

Incubation area for experiments that can later be moved into a production crate
or project, along with their tests, fixtures, and provenance. See [AGENTS.md](AGENTS.md).

## Current lab

[blender-godot-sqlite-proof](blender-godot-sqlite-proof/) contains:

- Rust collision, rollback, and SQLite experiments in `core-labs`.
- A separate wgpu trace consumer in `render-lab`.
- Blender source assets, Godot projects, and MP4 evidence.
- [Executed results](blender-godot-sqlite-proof/18_results.md).
- [Fighter-domain library inventory](blender-godot-sqlite-proof/research/10_smash_ecosystem.md).
- [Native PM Falcon knee fixture](blender-godot-sqlite-proof/falcon-lab/7_readme.md):
  extracted idle/jump/knee poses, Parry contact, target damage, wgpu MP4.
- [Input and rollback increment](blender-godot-sqlite-proof/falcon-lab/14_rollback_readme.md):
  two GGRS peers, a delayed attack, state-labeled side-by-side capture.
- [Sandbag launch increment](blender-godot-sqlite-proof/falcon-lab/20_launch_readme.md):
  ssbm_utils launch/hitstun, Rapier flight/landing, complete physics rollback.
- [SQLite presentation increment](blender-godot-sqlite-proof/falcon-lab/26_sql_readme.md):
  recycled numeric pose rows, atomic rollback correction, pinned SQL reader, wgpu capture.
- [gdext presentation increment](blender-godot-sqlite-proof/falcon-lab/33_godot_readme.md):
  Godot consumes the same Rust/SQL fixture, with exact row and mesh-upload checks.
- [Incremental core](blender-godot-sqlite-proof/falcon-lab/43_incremental_readme.md):
  renderer-free Rust state, one-call advances, full-state replay and save/load checks.
- [Independent schedules](blender-godot-sqlite-proof/falcon-lab/52_schedule_readme.md):
  worker continues through paused consumption and exhausted SQL slots, then publishes corrected history.

The Rust packages retain separate Cargo workspace roots and lockfiles. Parent
`hafley-rs` workspace builds do not include these experimental dependencies.

From this directory:

```sh
cargo test --locked --manifest-path blender-godot-sqlite-proof/core-labs/Cargo.toml
bash blender-godot-sqlite-proof/17_run_labs.sh
bash blender-godot-sqlite-proof/wireframe-fighter/5_run.sh
```

Rendering commands require Blender, Godot, and ffmpeg on PATH. Ozz remains an
optional blocked experiment; see the executed-results document for compiler scope.

## What has been established

Eight component tests cover capsule collision, local Rapier snapshot/replay,
snapshot independence, GGRS mismatch detection, and bounded/recycled SQLite
storage. Godot and wgpu recordings consume the same Rust-generated trace. A
separate 16-bone Blender fighter imports and animates in Godot.

Remaining integration work includes a combined animated fighter simulation,
networked rollback, cross-platform determinism, and validating the newly found
domain-specific dependencies. These results do not establish those properties.

Moved from `/Users/chrishafley/projects/blender-godot-sqlite-proof` on 2026-09-07.
Existing repositories under `/Users/chrishafley/projects/games` were left in place.
