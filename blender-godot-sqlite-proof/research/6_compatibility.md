# Compatibility matrix

Source review only. Exact lockfile resolution and builds remain test gates.

| Component pin | role / coupling | math and state boundary | version, target, license constraints |
| --- | --- | --- | --- |
| Rapier 0.35.3, `b82079a` | Engine-independent 2D/3D collision/controller/dynamics | `f32` default, separate `-f64` crates; current aliases use the Rapier/glamx math surface. Full rollback wrapper serializes durable sets and app state, omitting scratch pipelines per its guide. | `enhanced-determinism` required for cross-platform contract; `simd8` excluded; same initial state, insertion order, crate, compiler, and machine conditions still apply. License/MSRV/target build at this exact pin `[UK]`. |
| ozz-animation-rs 0.11.0, `ece37e90...` | Engine-independent sampling/blending/hierarchy | `f32` SoA transforms and matrices. Adapter copies into a canonical `[f32; 16]` column/row convention selected by tests. Snapshot explicit animation phase/blends/root accumulator/events; rebuild or restore sampling cache only after equivalence test. | Rust 1.88, nightly portable SIMD, MPL-2.0, default serde/rkyv. README cross-platform determinism is upstream-reported; bitwise x64/ARM64/Wasm equality `[UK]`. |
| GGRS v0.13.0, tag SHA `[UK]` | Engine-independent rollback/session | Application `State: Clone`, checksum, save/load/advance. Snapshot contains every outcome-affecting value. | Crate bounds, MSRV, license, socket features, allocator behavior, and v0.13.0 SHA `[UK]`; pin before build. |
| rusqlite 0.40.2 + bundled SQLite pin `[UK]` | Persistence/query adapter, outside deterministic step | Scalars and canonical byte blobs; virtual cursors pin immutable ring generations. | `vtab` feature; bundled/system SQLite choice changes runtime version and compile options. Threading follows connection and module state. rusqlite/SQLite licenses and target availability must be recorded from resolved manifests/build. |
| godot 0.5.5 | Godot presentation adapter | Godot `Vector*`/`Transform*` conversion only at host boundary. `double-precision` must match a Godot `precision=double` build. | Godot 4.2 minimum; select exactly one API level no newer than runtime. MPL-2.0. Mobile/Wasm/thread support is experimental upstream. |
| winit 0.30.13 + wgpu 30.0.1 | Second presentation adapter | Canonical POD matrices/instances become GPU bytes; no renderer types enter simulation. | Native backend and Web/Wasm matrix needs pinned builds. wgpu is MIT/Apache-2.0; winit license/MSRV at exact pin `[UK]`. |

## Required conversions

1. Rapier pose/vector to canonical simulation `SimVec`/`SimPose`. Prefer the
   same `f32` precision end-to-end; an `f64` core would narrow for ozz/GPU/Godot
   default builds and changes checksum policy.
2. Ozz SoA local transforms to model matrices, then canonical `Mat4` with a
   tested handedness, axis, matrix-major, inverse-bind, and multiplication-order
   convention. Godot and WGSL receive separate adapter conversions.
3. GGRS snapshots serialize stable IDs and ordered dense arrays. Hash maps,
   pointer addresses, host handles, SQL connections, GPU resources, and scratch
   pipelines stay outside.
4. SQLite rows/blobs use explicit little-endian schema/version fields. Database
   reads never become the authoritative state during a rollback step.

## Compatibility gates

Proposed, not executed: resolve a single lockfile; compile desktop x86_64 and
ARM64; compile Godot against the selected 4.x editor/export templates; run a
600-frame replay in both hosts; compare snapshot sizes and hashes; verify
Rapier `enhanced-determinism` without incompatible SIMD; test `.ozz` converter
and runtime pin together; assert matrix/bone fixtures in both hosts; enumerate
licenses with the resolved dependency graph. Web, Android, iOS, and consoles
remain `[UK]` until target builds and runtime tests pass.
