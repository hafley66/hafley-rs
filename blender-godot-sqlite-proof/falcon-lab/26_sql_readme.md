# Recycled SQLite presentation boundary

Run `bash 22_run_sql.sh` from this directory. Requires ffmpeg and Metal GPU
access on this Mac. Cargo builds use two jobs; tests and recording run sequentially.
CPU-only verification: `cargo run -j 2 --locked --offline --bin falcon-rollback -- --sql --verify-only`.

Outputs: `23_sql_boundary.mp4`, `24_sql_boundary_trace.json`, `25_sql_frames.png`.
Earlier recordings remain unchanged.

Verified capture: H.264, 960x540, 824 frames, 13.733333 seconds, 731,187 bytes.
The run published 180 generations with a maximum of 450 retained rows.
All eight Falcon tests and eight core tests pass; Falcon Clippy passes with
warnings denied. Cargo reports an existing future-incompatibility notice for
the transitive `block` 0.1.6 dependency.

## Data path

The existing Rust/GGRS simulation emits numeric presentation rows after every
advance, including replay advances. A publication replaces replayed history in
one operation. The wgpu viewer queries SQLite for the current tick and reconstructs
capsules, attack spheres, and the target cuboid from those rows. Drawing receives
neither the physics world nor the decoded animation assets.

`core-labs::sql::FrameRing<Row>` owns three preallocated generations, each with
capacity for 1,024 rows. `Boundary` maintains a 32-tick window using two additional
preallocated row buffers. The SQLite in-memory connection exposes a read-only
virtual table over these Rust buffers. There are no schema indexes or per-tick
INSERT statements. Query planning currently performs a bounded scan.

A cursor pins its generation with an Arc at filter time. Publication takes the
ring write lock, chooses an unpinned slot, copies the complete window, and exposes
that generation. Pinned readers continue seeing their old rows. Publication
returns false when every slot is pinned or the row capacity would be exceeded.
The fixture asserts publication succeeds on every simulation tick.

Snapshot isolation applies to each virtual-table cursor. Independent queries or
multiple cursors in a join can pin different generations if a concurrent writer
publishes between their starts. The renderer uses one SELECT for its complete
frame. A future engine adapter must preserve that boundary or explicitly pin a
generation across its queries.

## Executed assertions

- At tick 91, a real SQL cursor reads predicted damage 0 and remains open.
- At tick 97, GGRS restores tick 78 and advances through tick 97. All 20 corrected
  frames are published together. Fresh tick-91 queries read damage 18.
- Corrected geometry, target state, and authoritative presentation fields match
  the on-time peer for ticks 78 through 97.
- At tick 101, the held cursor finishes with its original rows and generation 92.
  Its slot is subsequently reused. Every slot retains its allocation address and
  capacity throughout the run.
- SQL rows round-trip exactly, retained history stays within 32 ticks, writes to
  the virtual table fail, and the SQLite schema contains zero indexes.
- Tests separately hold all slots to verify refusal and recovery, and submit an
  oversized publication to verify the prior generation survives unchanged.

The MP4 labels current tick, published/rendered generations, action/pose, input
prediction, damage/hitstun, window size, rollback, and held versus fresh reads.
Playback uses two video frames per simulation tick plus labeled presentation holds.

## Numeric schema

Each row has `tick`, `kind`, `entity`, `generation`, and `v0` through `v23`.
Matrix elements are column-major. Positions use the existing PM fixture axes:
depth X, up Y, forward Z.

| kind | entity | Values |
| --- | --- | --- |
| 0: frame | 0 | v0 action; v1 zero-based pose; v2..4 root; v5 damage; v6 hit count; v7 last-hit tick or -1; v8 contact; v9 predicted; v10 input bits; v11..12 animation X/Y translation; v15 restored tick or -1; v16 advance count; v17 load count; v18 confirmed tick |
| 1: target | 0 | v0..2 position; v3..5 velocity; v6 hitstun; v7 phase; v8 grounded |
| 2: hurt capsule | bone index | v0..15 bone matrix; v16..18 endpoint A; v19..21 endpoint B; v22 radius; v23 enabled |
| 3: attack sphere | hitbox ID | v0..2 position; v3 radius; v4 damage; v5 enabled |

Unused values are zero. Target phases: hovering 0, hit 1, hitstun 2, falling 3,
landed 4. Views expose `frame_state`, `target_state`, `hurtboxes`, and `attacks`.

## Scope

Bounded/recycled storage refers to the publication ring and its window buffers.
This offline fixture still allocates GGRS snapshots, encoded per-advance rows,
the full audit trace, SQL query results, render meshes, and video frames. It does
not establish allocation-free execution or measured real-time performance.

The inherited PM pose/contact, Melee-derived knockback, scripted Falcon travel,
and Rapier landing limitations remain documented in `20_launch_readme.md`.
Godot consumption of this numeric schema, real network transport, cross-platform
determinism, full skeletal skinning, and production extraction remain untested.
