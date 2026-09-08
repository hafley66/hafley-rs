# gdext presentation adapter: narrow research

Source review, 2026-09-07. No adapter implemented or recording executed.
Installed runtime: `godot --version` returns `4.7.stable.official.5b4e0cb0f`.

Follow-up: the adapter and recording have since executed. See
[`33_godot_readme.md`](../falcon-lab/33_godot_readme.md). The sections below retain
the original research-stage proposals; the implementation uses a bounded Rust
worker bridge rather than owning the SQL connection on the Godot main thread.

## 1. Geometry and bones

Godot exposes `ArrayMesh::add_surface_from_arrays(primitive, arrays)` and
`surface_update_vertex_region(surface, offset, bytes)` through gdext.
Proposed wireframe adapter: reuse Parry capsule/sphere/cuboid tessellation, turn
triangle edges into line vertices, upload packed positions/colors to an ArrayMesh,
and display it with MeshInstance3D. Validate coordinates and matrix column order
against the existing wgpu consumer before adding camera differences.
[Godot ArrayMesh](https://docs.godotengine.org/en/stable/classes/class_arraymesh.html),
[gdext Rust signatures](https://docs.rs/godot/0.4.5/godot/classes/struct.ArrayMesh.html).

The existing numeric rows provide capsule endpoints/radius, associated bone
matrices, attack spheres, target position, and root/animation translation. These
cover the current wireframe fixture. The existing Godot capsule helper in
`../godot/9_render_collision.gd` also establishes endpoint-to-capsule placement.

For later skinning, `Skeleton3D::set_bone_pose` and `set_bone_global_pose` exist.
Godot documents global pose as skeleton-relative and warns that repeated global
pose setters can trigger recalculation. The current SQL rows contain transforms
for hurtbox-associated bones, without a complete bone hierarchy, rest/bind data,
or mesh weights. Those static asset mappings must be supplied and validated
before treating the rows as a full skinning palette. No additional skinning data
is required for this capsule-only adapter test.
[Skeleton3D](https://docs.godotengine.org/en/stable/classes/class_skeleton3d.html).

## 2. Acquire, copy, release

Existing Rust signature:

```rust
fn read_frame(db: &Connection, tick: i64) -> Result<(u64, Vec<Row>)>;
// One SELECT pins a generation, collects that tick's numeric rows,
// verifies their generation agrees, and releases statement/cursor on return.
```

Proposed adapter signature, not implemented:

```rust
fn present(&mut self, generation: u64, rows: &[Row]);
// Convert the owned frame rows into Godot geometry and state labels.
```

For the first fixture, a main-thread gdext node owns the Rust fixture and SQL
connection for its scene lifetime. Each scheduled simulation step advances Rust,
publishes the completed replay window, reads one SQL frame, then calls `present`.
Godot callbacks provide presentation opportunities; the fixture's tick/hold
schedule determines when to advance. Cleanup drops readers before their owner.
[gdext INode lifecycle](https://docs.rs/godot/latest/godot/classes/trait.INode.html).

The cursor Arc pins the source allocation only during the read. The returned Vec
owns copied rows, so GPU/render stalls after copying do not pin that ring slot.
The existing read_frame allocates; buffer reuse in the adapter remains an
implementation/measurement task. Keep the deliberately held test cursor on a
separate connection through ticks 91..101. A new SELECT can see a newer
generation, including within a multi-cursor query; this virtual table does not
provide transaction-wide generation pinning. Keep the frame read to one cursor.

A threaded live simulation is outside this increment. If added later, each
reader needs its own connection and bounded publication failure needs an explicit
retry/drop policy. The current fixture asserts publication success.

## 3. Comparable capture

Reuse `../10_record_collision.sh`: Godot Movie Maker with `--write-movie` and
`--fixed-fps`, followed by ffmpeg H.264/yuv420p/faststart encoding and ffprobe.
This existing script captures AVI as an intermediate. Fixed-rate recording is
documented by [Godot Movie Maker](https://docs.godotengine.org/en/stable/tutorials/animation/creating_movies.html).

Proposed acceptance: repeat the current 180-tick delayed-input fixture, with the
same 824-frame display schedule at 60 FPS. Assert exact consumed rows and
generations against a reference produced by the shared Rust fixture before
Godot coordinate conversion. Check ticks 91, 97, 101, and 126 for prediction,
atomic correction, reader release, and landing. Retain the held-reader and
allocation-reuse assertions. Compare converted endpoints/transforms numerically;
inspect visibility and captions in images. Pixel identity across renderers is
not an acceptance condition. Produce a state-labeled MP4 and comparison report.

## Remaining compile gate

Earlier `5_hosts.md` names godot 0.5.5. The fetched docs.rs ArrayMesh `latest`
page resolves to 0.4.5. This review verifies API availability in that documented
binding, without establishing the current release pin. Before implementing,
resolve an exact published gdext version/API feature compatible with installed
Godot 4.7 and compile a load-only extension. Then implement only the SQL-to-
wireframe adapter above. No engine survey or simulation redesign is required
by this experiment.
