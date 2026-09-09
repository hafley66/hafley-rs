# Reused game libraries

Source: sibling `hafley-rs-game-runtime` at commit `8646fa2`, MIT OR Apache-2.0.
The `crates/redux` and `crates/rollback` source files were read from that checkout
on 2026-09-08. Those crate files had no worktree modifications. Application code
was not copied or changed. These local ports let the active lab build without
depending on a sealed application checkout. Further active-lab changes belong here;
no synchronization or modification of the source checkout is implied.

- `redux`: existing Slice/Then/Zoom/Each reducer algebra, Store/replay, deterministic
  Pool/FreeSpans, and original tests. Pure simulation ticks implement Slice.
- `rollback`: existing RollbackSim/Game/GgrsConfig and sync-test construction.
  Shared apply_request executes save/load/advance for both Game and the Falcon
  lab's instrumented request observer. Presentation stays outside World.

The Falcon rollback golden and fault tests must remain green after changing these
boundaries. A source port alone does not establish behavior preservation. Browser
transport and cross-target network play remain separate verification gates.

Current user scope: site-wide acquisition is deferred in Falcon `104_tasks.md`; import
Falcon's available source data with unsupported common callbacks reported; model
snapshot-owned stage terrain independently of rendered geometry. Site decoding
must remain offline. Do not replace these libraries with new reducer/session code.

## Input and capture slice (2026-09-09)

- `input`: macro-free statig 0.4.1 lane buffer. `Buffer::advance(press, eligible,
  cancel, window) -> Outcome` mutates snapshot-owned state; `remaining()` only
  inspects it. The game owns dispatch/aging, eligibility, cancellation and window.
  V1 age-before-record/newest-press semantics are distilled from
  `games/kneeman/src/v1/{state,fighter,za_warudo}.rs`; window N admits the press
  tick plus N following ticks. No entry/exit actions, so Serde reinitialization
  is inert. Falcon's existing Redux Step consumes this implementation.
  `PlayerInput` and quantization were ported from `crates/input` at `8646fa2`;
  the Falcon adapter still accepts its existing button bits. Four-axis controller
  integration and V1 tech/coyote buffers remain pending.
- `capture`: existing Falcon wgpu/FFmpeg recorder and shader moved here.
  `frame_checked(vertices, verify_rgba)` borrows a mesh for one synchronous
  render/readback/encode step. Capture owns GPU resources and the encoder until
  `finish`. Fixed 960x540, 60 fps H.264, two encoder threads; per-frame upload
  allocation is unchanged. Falcon pixel assertions remain in `1_gpu.rs`;
  the buffer comparison supplies its own panel assertions. Receipt/deployment
  automation extraction remains pending. Neither crate installs a subscriber.

Buffer policy and inspection payloads are game-owned TypeSpec models generated
by the existing emitter. This slice does not change the emitter implementation.
