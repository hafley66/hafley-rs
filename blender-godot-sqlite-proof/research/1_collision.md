# Collision, movement, and physics research, shot A

As of 2026-09-07. This is a source review. No tests were run in this
workspace.

Evidence labels used below:

- **[DG] documented guarantee**: stated in package/API documentation or an
  upstream specification.
- **[SO] source observation**: visible in source, manifest, changelog, or
  package contents.
- **[TR] reported test result**: a test result reported by an upstream
  maintainer. It was not reproduced here.
- **[IN] inference**: consequence of the documented API boundary.
- **[UK] unknown**: requires a pinned build or experiment.

## Candidate matrix

| Candidate and pin | Queries | Controller | Dynamics | Snapshot and determinism | Reuse, ownership, constraints |
| --- | --- | --- | --- | --- | --- |
| [Rapier 0.35.3, release commit `b82079a`](https://github.com/dimforge/rapier/commit/b82079a) | `QueryPipeline` and `QueryPipelineMut` provide ray, shape-cast, intersection, and related spatial queries. | `KinematicCharacterController::move_shape`; optional approximate collision impulses. | `PhysicsPipeline::step` over caller-provided bodies, colliders, broad/narrow phase, islands, joints, and CCD state. | `serde-serialize` serializes simulation structures; the Rust user guide says to omit pipelines from a full-state wrapper. Determinism has exact-initial-state, version, compiler, machine, feature, and floating-point conditions. | Pipeline scratch memory is reusable. App owns the sets, character pose, input, callbacks, and gameplay state. 2D and 3D crates, plus `f64` variants; math API migrated toward `glamx`/`Pose`/`Vector` in the 0.32 line. |
| [Parry 0.30.2, release commit `1be4b1a`](https://github.com/dimforge/parry/commit/1be4b1a) | Geometric intersection, distance, closest points, contacts, linear casts, and nonlinear casts through `QueryDispatcher`. | None. | None. | `serde-serialize` and `rkyv` serialize Parry data types. A world snapshot and determinism contract require application-owned state around those values. | Caller owns shapes, poses, BVH/partitioning, outputs, and any temporal cache. `BvhWorkspace` is reusable temporary storage. 2D, 3D, and `f64` crates; no engine or ECS dependency. |
| [boxdd 0.5.0, release commit `a3d1e2a`](https://github.com/Latias94/boxdd/releases/tag/v0.5.0) | World overlap, ray, shape, and cast queries plus standalone geometry helpers. | Box2D 3.1 geometric mover surface: `cast_mover`, `collide_mover`, and plane-solving helpers. | FFI wrapper for Box2D v3 worlds, bodies, shapes, joints, contacts, events, and `World::step`. | Wrapper `serialize` feature provides save/apply configuration and a minimal full-scene wrapper snapshot. Native Box2D exposes caller-buffer snapshots, state hashes, and recording; verify which functions the wrapper exposes. | `*_into` and visitor APIs support caller buffers and scoped views. `World` and owned handles are `!Send`/`!Sync`; worker callbacks can run on worker threads when task callbacks are configured. Core is engine-independent but uses vendored or system C through `boxdd-sys`; `bevy_boxdd` is a separate adapter. 2D only. |
| [box2d-rust 1.3.0](https://docs.rs/crate/box2d-rust/1.3.0), release SHA **unknown** | Ported Box2D world queries, overlap, ray/shape casts, and collision geometry. | Ported Box2D character mover API in the `mover` module. | `world_step(&mut World, time_step, sub_step_count)` over an owned Rust `World`; bodies, shapes, contacts, joints, solver sets, and islands are included. | README reports `world_snapshot`, `world_restore`, deep state hash, and full operation recording. It reports bit-exact comparison against its pinned C reference. | Serial Rust implementation; README says C task system, global world registry, and C arena allocator were not ported, with Rust `Vec`s used instead. `World` is an owned value and IDs are explicit. Package 1.3.0 is dated 2026-07-23, but the fetched package did not expose its release SHA. |

The four entries cover separate layers. Parry is a geometry/query substrate.
Rapier composes a query substrate with a character controller and a full rigid
body pipeline. boxdd and box2d-rust cover a Box2D-style 2D world, including a
geometric mover and dynamics. Controller movement output and dynamic-body
integration are separate phases in each API.

## Rapier 0.35.3

### API layers

- **Query.** [`QueryPipeline<'a>`](https://docs.rs/rapier3d/0.35.3/rapier3d/pipeline/struct.QueryPipeline.html)
  is a borrowed query view with a dispatcher, BVH, `RigidBodySet`,
  `ColliderSet`, and `QueryFilter`. It supports raycasts, shape casts, and
  intersections. [`QueryPipelineMut`](https://docs.rs/rapier3d/0.35.3/rapier3d/pipeline/struct.QueryPipelineMut.html)
  has mutable body and collider references for APIs requiring mutable access.
  **[DG]**
- **Controller.** [`KinematicCharacterController`](https://docs.rs/rapier3d/0.35.3/rapier3d/control/struct.KinematicCharacterController.html)
  takes `dt`, a query view, a caller-supplied shape and pose, a desired
  translation, and a collision callback. It returns
  `EffectiveCharacterMovement { translation, grounded,
  is_sliding_down_slope }`. The documented example applies the returned
  translation to the caller's pose. `solve_character_collision_impulses` can
  apply approximate impulses to surrounding rigid bodies through
  `QueryPipelineMut`. **[DG]**
- **Dynamics.** [`PhysicsPipeline::step`](https://docs.rs/rapier3d/0.35.3/rapier3d/pipeline/struct.PhysicsPipeline.html)
  receives mutable `IslandManager`, `BroadPhaseBvh`, `NarrowPhase`, body and
  collider sets, joint sets, and `CCDSolver`, plus hooks and an event handler.
  It performs collision detection, solving, integration, and position
  correction. `RigidBodyType` distinguishes dynamic, fixed,
  kinematic-position-based, and kinematic-velocity-based bodies. **[DG]**
- **Collision-only.** [`CollisionPipeline`](https://docs.rs/rapier3d/0.35.3/rapier3d/pipeline/struct.CollisionPipeline.html)
  is the separate broad and narrow collision pipeline for applications that do
  not want force integration or constraint solving. Reuse of this pipeline's
  temporary buffers is documented in the same API family. **[DG]**

### State ownership, lifetime, and reuse

`PhysicsPipeline` stores temporary working memory; the docs state that reusing
one instance lets Rapier reuse allocations. The durable simulation state is in
the sets and pipeline components passed to `step`. **[DG]** The caller also
owns input, gameplay state, controller configuration, character pose, custom
hooks, event collection, and renderer synchronization. The borrowed query
pipeline cannot outlive the referenced body, collider, and BVH values. **[IN]**

Rapier has `rapier2d`, `rapier3d`, `rapier2d-f64`, and `rapier3d-f64` package
families. The 0.32 migration changed the math surface from the prior nalgebra
forms toward `glamx` and aliases such as `Pose` and `Vector`; adapters should
pin the exact crate and feature set. **[SO]**

### Serialization and determinism

The [serialization guide](https://rapier.rs/docs/user_guides/templates/serialization/)
says the `serde-serialize` feature makes most Rapier data structures
serializable. A full Rust state wrapper should serialize every simulation
structure used by the application, while omitting `PhysicsPipeline` and
`CollisionPipeline` because they contain no useful durable state. The wrapper
must also carry app-owned input, character pose, controller configuration,
gameplay state, and any external callback state. **[DG]**

The [determinism guide](https://rapier.rs/docs/user_guides/templates_injected/determinism/)
documents local determinism when the initial conditions, construction and
insertion order, machine, Rapier version, and Rust compiler match. Cross-
platform determinism additionally requires `enhanced-determinism`, excludes
`simd8`, uses IEEE 754-2008 behavior, and avoids platform-dependent
transcendental initialization. **[DG]** The contract excludes host and callback
state unless those inputs also obey the same conditions.

### Maintenance and constraints

The 0.35.3 package was published 2026-08-28. The release commit shown by
GitHub is `b82079a` (short hash; obtain the full hash from the lockfile or Git
ref before a final pin). The manifest exposes `serde-serialize`,
`enhanced-determinism`, `parallel`, and `simd8`-related choices. The Parry 0.30
SIMD feature changes make `simd8` incompatible with enhanced determinism, so
the feature matrix belongs in the compatibility audit. **[SO] [UK]**

## Parry 0.30.2

### API layers

Parry is a 2D/3D geometric library with `shape`, `query`,
`bounding_volume`, `partitioning`, and mass-property modules. It has no world,
rigid-body set, force solver, controller, or engine host. **[SO] [IN]**

[`QueryDispatcher`](https://docs.rs/parry3d/0.30.2/parry3d/query/trait.QueryDispatcher.html)
dispatches `intersection_test`, `distance`, `closest_points`, `contact`,
linear `cast_shapes`, and nonlinear `cast_shapes_nonlinear`. The methods return
`Result<..., Unsupported>` when the shape pair or operation is unsupported. The
dispatcher is `Send + Sync`; a custom dispatcher can be chained with another
dispatcher. **[DG]**

The [linear cast API](https://docs.rs/parry3d/0.30.2/parry3d/query/fn.cast_shapes.html)
takes two poses, two linear velocities, two shapes, and `ShapeCastOptions`,
returning an optional `ShapeCastHit` with time-of-impact data. Nonlinear casts
use a `NonlinearRigidMotion` for translation and rotation. Persistent contact
queries accept caller-owned output vectors and optional workspaces, allowing
temporal manifold storage to be explicit. **[DG]**

### State ownership, lifetime, and reuse

The application owns shape values, poses, object IDs, broad-phase leaves,
`Bvh`, `BvhWorkspace`, result containers, and any pair/manifold cache. This
follows from the functions taking references and returning geometric values
without a world object. **[IN]** A
[`BvhWorkspace`](https://docs.rs/parry3d-f64/0.30.2/parry3d_f64/partitioning/struct.BvhWorkspace.html)
holds temporary buffers for refit, rebuild, and optimization. The docs state
that it grows for the largest operation, does not shrink automatically, and can
be dropped and recreated to release memory. **[DG]**

The `alloc` feature gates heap-backed structures such as `smallvec`,
`hashbrown`, `downcast-rs`, `rstar`, and related support. `serde-serialize`,
`rkyv`, and `bytemuck-serialize` provide different data boundaries. Individual
queries may still allocate for composite-shape traversal or caller-selected
result containers; the reviewed sources document no global allocation-free
guarantee. **[SO] [UK]**

### Serialization, determinism, and maintenance

Parry offers serialization features for its data types. It does not own a world,
so a whole-world snapshot must be assembled by the application. If a rollback
state includes a BVH or persistent manifold workspace, the application must
decide whether and how to serialize those values. The
[changelog](https://github.com/dimforge/parry/blob/master/CHANGELOG.md) records
a 0.29.0 fix so enhanced-determinism
also disables architecture-specific SIMD in `parry3d` and `parry3d-f64`, and a
0.26.1 fix that moved incremental BVH optimization state into serializable
`Bvh` rather than nonserializable workspace. The 0.30.0 changelog removes older
SIMD feature names and documents `simd8` as incompatible with
`enhanced-determinism`. **[SO]**

The package was published 2026-08-07. GitHub shows release commit `1be4b1a`
(short hash; full hash needs lock/ref verification). Parry is the geometry
dependency underneath the selected Rapier release, so using it directly avoids
Rapier's durable body and solver state but requires an application-owned
world/index/controller boundary. **[SO] [IN]**

## boxdd 0.5.0

### API layers

The [0.5.0 crate documentation](https://docs.rs/boxdd/0.5.0/boxdd/) describes
a safe Rust layer over the Box2D v3 C API. The core crate exposes world, body,
shape, joint, query, event, debug-draw, geometry, mover, and serialization
modules. The separate `bevy_boxdd` package is an adapter and is outside this
candidate. **[SO]**

- **Query.** World overlap/ray/shape casts are available along with standalone
  `ShapeProxy`, `SimplexCache`, `DistanceInput`, `ShapeCastPairInput`, `Sweep`,
  `ToiInput`, `segment_distance`, `shape_distance`, `shape_cast`, and
  `time_of_impact` helpers. **[DG]**
- **Controller.** Box2D's geometric mover surface is represented by
  `cast_mover`, `collide_mover`, `collide_mover_into`, `solve_planes`,
  `try_solve_planes`, and `clip_vector`. The upstream
  [character documentation](https://box2d.org/documentation/md_character.html)
  describes this as an experimental geometric mover, separate from a rigid
  body, with a capsule-oriented input model and no explicit rotation. **[DG]**
- **Dynamics.** `World` owns the Box2D simulation handle and exposes bodies,
  shapes, joints, contacts, events, and stepping. Box2D's `WorldDef` supports a
  worker count; callbacks may execute on worker threads when task callbacks are
  configured. **[DG]**

### State ownership, lifetime, allocation, and threading

boxdd exposes owned handles whose `Drop` destroys the underlying object,
scoped borrowed views such as `Body<'_>`, and explicit IDs such as `BodyId`,
`ShapeId`, `JointId`, and `ChainId`. Owned event snapshots are safe across
steps. `*_into` getters reuse caller-owned storage, and `with_*_events_view`
visitors expose closure-scoped slices. Raw slices have stricter unsafe lifetime
rules tied to the completed step and deferred-destroy flush. **[DG]**

The docs mark `World` and owned handles `!Send`/`!Sync`, so the app must keep a
world on one thread or use a dedicated physics thread and message boundary.
The `worker_count` setting does not change those auto-traits. **[DG]**

Ordinary event/query getters return owned collections and can allocate;
`*_into` and visitor forms expose the reuse boundary. The native Box2D header's
[`b2World_GetMaxCapacity`](https://github.com/erincatto/box2d/blob/main/include/box2d/box2d.h)
documents pre-reserved capacity for avoiding runtime allocations and copies.
The current `main` header is an unpinned source for the boxdd 0.5.0 vendored C
revision. **[DG] [UK]**

### Snapshot and determinism boundary

The boxdd `serialize` feature documents save/apply world configuration and a
minimal full-scene wrapper snapshot. It is wrapper-owned serialization; native
Box2D internal-byte coverage remains a separate question. The docs
identify wrapper-created chain and shape metadata as part of that boundary;
verify coverage for objects created through lower-level paths. **[DG] [UK]**

Native Box2D's official header documents a separate full simulation snapshot
API, a deep state hash, and recording. Snapshots are taken between steps into a
caller-provided buffer, preserve same-world IDs for objects present in the
snapshot, and omit host wiring such as task callbacks, worker count, userdata,
and filter/material callbacks. The header's state hash covers transforms,
velocities, impulses, and index bookkeeping while ignoring padding/free slots.
These native guarantees apply to the exact C API revision and require
verification against the calls exposed by boxdd 0.5.0. **[DG] [UK]**

The boxdd release page dates 0.5.0 to 2026-07-06 and shows release commit
`a3d1e2a` (short hash; full hash needs lock/ref verification). The FFI layer
uses vendored or system C sources and a C compiler, which is a target/build
constraint for a cross-host Rust foundation. **[SO]**

## box2d-rust 1.3.0

### API layers and ownership

The [1.3.0 package](https://docs.rs/crate/box2d-rust/1.3.0) is a pure Rust
port of Box2D v3.1. The crate root reexports explicit IDs and has modules for
world, mover, collision, recording, and geometry. The
[`world_step`](https://docs.rs/box2d-rust/1.3.0/box2d_rust/world/fn.world_step.html)
signature takes `&mut World`, `time_step`, and `sub_step_count`, with an
owned-value `World` rather than a C global world registry. **[SO] [DG]**

- **Query.** World query/cast functions and standalone collision geometry are
  part of the ported API, including AABB, distance/GJK/TOI, hull, manifold,
  and dynamic-tree modules. **[SO]**
- **Controller.** The `mover` module ports the Box2D character mover and the
  README lists character movers among the covered upstream sample categories.
  It is a geometric movement phase; its output must be applied by the caller.
  **[SO] [IN]**
- **Dynamics.** The world, body/shape/contact lifecycles, constraint graph,
  solver sets, islands, joints, sensors, sleeping, continuous collision, and
  serial step pipeline are included in the port's coverage table. **[SO]**

The application owns its input stream, gameplay state, render transforms, and
the `World` value or a containing simulation state. Explicit IDs and the
`IdPool` API make object identity part of the serial Rust state. The README
states that the C task system, global world registry, and C arena allocator
were not ported; Rust `Vec`s are used instead. **[SO]**

### Snapshot, recording, determinism, and provenance

The [package README](https://docs.rs/crate/box2d-rust/1.3.0/source/README.md)
reports `world_snapshot`/`world_restore`, a deep state hash,
and full operation recording/replay. It also reports bit-exact FallingHinges
state hashes against its pinned C reference, with hand-rolled deterministic
trigonometry and preserved `f32` operation order. These are upstream-reported
results, marked **[TR]**, and were not run here.

The package was published 2026-07-23. The fetched package contents did not
expose `.cargo_vcs_info.json` or a release commit. Its bundled `todo.md` records
the last visible source snapshot as `main` `e77b58b` at v1.2.0 and the C
reference submodule as `56edae7`; neither is a verified 1.3.0 release pin.
**[SO] [UK]** Resolve the 1.3.0 Git ref or crate provenance before using it as
a rollback dependency.

The port is serial by design, so there is no worker-count choice to reproduce.
The Rust `Vec` model means capacity growth and retained buffers must be measured
for the intended scene. The README reports the C suite's 132 tests green in
both precision modes, but this remains a maintainer report. **[TR] [UK]**

## Application-owned rollback boundary

Across the candidates, the rollback record must be wider than a body transform
array. It should explicitly identify:

- fixed-tick input and command order;
- object creation/removal order and stable IDs;
- dynamic body state, contacts, joints, solver/island state, and broad-phase
  state when required by the selected dynamics library;
- character pose, desired movement, grounded/sliding output, controller config,
  and any mover-specific temporal cache;
- query/manifold caches if they affect subsequent decisions;
- RNG, callbacks, filters, material rules, worker configuration, and other
  host wiring that the library snapshot omits;
- animation/gameplay state and the presentation engine's transform/skeleton
  buffers, which remain outside the physics library.

For Rapier, a serializable app wrapper is required and pipeline scratch objects
are reconstructed. For Parry, every world/index/cache is app-owned. For boxdd,
the wrapper snapshot and native snapshot APIs must be distinguished. For
box2d-rust, world snapshot/hash/recording are exposed by the port, with release
provenance still unresolved. **[IN] [UK]**

## One proposed experiment, not run

Build one headless Rust harness with a fixed 2D scene and separate lanes for
Rapier2d 0.35.3, Parry2d 0.30.2, boxdd 0.5.0, and box2d-rust 1.3.0. The
Rapier lane uses `serde-serialize` and `enhanced-determinism`, with `simd8`
disabled. Use `dt = 1/60`, fixed creation/insertion order, a static floor and
wall, two dynamic boxes, one sensor, and a capsule mover. Feed the same 120-tick
integer input stream to each lane. Parry runs the same geometry/query cases but
has no dynamics lane.

At tick 60, capture each lane's available state, advance 60 ticks, restore,
and replay the same inputs. Assertions:

- canonicalized body transforms, velocities, and object IDs match the first
  run at every replay tick;
- contact/event sequence and query hit ordering match;
- mover translation, grounded, slope, and collision outputs match;
- within one build, the serialized snapshot or state hash is identical after
  the same ticks. Assert byte equality within each lane; compare semantic fields
  across different libraries.

Measurements:

- allocations and deallocations per tick using a counting allocator;
- retained capacities and peak capacities for pipeline, BVH, Vec, event, and
  snapshot buffers;
- snapshot byte size, capture/restore time, and one-step time;
- query/controller output counts and event order;
- thread mode and compiler/feature/target metadata.

If a host-rendered extension is added, log transforms from the actual harness,
render the scene, encode an H.264 MP4, and inspect frames for floor contact,
wall sliding, mover step, and rollback replay. The headless assertions alone
have no visible behavior and require no MP4. This experiment is proposed only;
it was not run.

## Brief exclusions

- `nphysics` and `ncollide` documentation describes them as passively
  maintained and superseded by Rapier/Parry. They were not shortlisted.
- `collision-detection` 0.8.1 and `collide` 0.7.0 provide collider/BVH query
  managers, but the reviewed APIs expose no character mover, rigid-body
  dynamics, snapshot contract, or determinism evidence. Their result maps and
  vectors can allocate despite README-level zero-allocation wording, so they
  were not shortlisted.
- Bevy-specific physics and controller adapters, including Avian and Tnua,
  are excluded by the brief. The core `boxdd` crate was reviewed separately
  from its `bevy_boxdd` adapter.
