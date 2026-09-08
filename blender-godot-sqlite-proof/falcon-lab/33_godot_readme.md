# gdext SQL presentation increment

The gdext library builds and loads into Godot 4.7. All 180 presentation ticks
pass exact SQL-row and uploaded-mesh-vertex roundtrip assertions. Publication
diagnostics match the saved wgpu recording trace, including held-reader state,
rollback correction, and bounded slot reuse. The capture uses actual Godot
ArrayMesh rendering through the OpenGL Compatibility backend on Apple M2 Pro.

Verified final MP4: H.264, 960x540, 824 frames, 13.733333 seconds, 626,502 bytes.
Nine Falcon tests and eight core tests pass. Clippy passes for all Falcon targets
with `gdext` enabled and warnings denied; the generated godot 0.4.5 initializer
requires a documented `redundant_field_names` lint allowance. Cargo retains an
upstream future-incompatibility notice for `block` 0.1.6.

## Implementation

`28_extension.rs` adds a feature-gated gdext library to the existing isolated
Falcon package. Pin: `godot = 0.4.5`, minimal code generation, `api-4-5`.
The cached binding manifest lists MPL-2.0. Runtime inspected:
`4.7.stable.official.5b4e0cb0f`. The runtime load test prints `GDEXT_LOAD_OK`.
The documented compatibility rule permits a newer runtime than the compiled
API: https://godot-rust.github.io/book/toolchain/compatibility.html.

The existing Rust fixture/SQL verifier accepts a consumer callback. A Rust worker
owns the decoded fixture, SQL connection, and deliberately held cursor. Two
capacity-one channels connect it to the Godot main thread. Only ordinary Rust
data crosses the thread boundary; Godot objects remain on the main thread.

Godot requests one frame at each simulation-tick boundary. The worker publishes
and verifies the SQL window and yields copied rows plus world-space wire geometry.
It can prepare the following publication before the next request; the displayed
generation labels describe the consumed snapshot. There is one pending packet
and capacity-one request/response channels.
`27_geometry.rs` supplies the same Parry tessellation/edge conversion to wgpu and
gdext. The adapter maps PM coordinates `(depth, up, forward)` to Godot
`(forward, up, -depth)`. Godot uploads positions/colors to an ArrayMesh line
surface. Its read-back vertex array and original rows are returned to Rust for
exact comparison before the next tick is requested.

The simulation trace is still computed offline before presentation begins, as
in the existing wgpu fixture. SQL generations are published during consumption.
This does not establish a continuously running or networked Godot-hosted game.

The worker preserves the existing real SQL cursor across ticks 91..101,
including rollback at 97. Normal frame queries release their cursor before data
reaches Godot. The deliberate held reader remains pinned on the worker stack.
Dropping the bridge closes its request channel and joins the worker. Successful
completion requires 180 acknowledged frames and all existing SQL assertions.

## Verification command

```sh
bash 29_run_godot.sh
```

The runner uses two compiler jobs, serial tests, an extension load test, then
Godot Movie Maker and ffmpeg. It clears the inherited launching application's
custom linker variable for this process only. Godot requires app-data access
even in headless mode; GPU capture also requires access to the graphics session.
Runtime loading is explicit, so an editor import and pre-existing `.godot` cache
are unnecessary. The headless editor crashed during shutdown in an initial
import attempt; the runtime load test and game capture exited normally.

Movie Maker emitted 825 frames despite reporting 824 scheduled frames. The
extra frame duplicates the final state. The encoder retains the first 824 frames,
matching the existing wgpu display schedule. Both the first and final frames and
the event montage were inspected.

Outputs:

- `30_godot_consumed.json`: per-tick rows/generation, row digest, mesh counts,
  exact roundtrip assertions, and held/fresh reader state.
- `31_godot_sql.mp4`: 960x540 H.264, 824 display frames for the 180-tick fixture.
- `32_godot_frames.png`: predicted state, correction, reader release, landing.
- `34_godot_sql.avi`: ignored Movie Maker intermediate.

The test retains the previous mixed PM-data/Melee-knockback/Rapier rules and
scripted Falcon movement. It does not add full skinning, asset import, production
crate extraction, performance guarantees, or allocation-free rendering.
