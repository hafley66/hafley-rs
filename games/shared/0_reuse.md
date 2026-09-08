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

Pending user scope: mirror all reachable Rukaidata site assets resumably; import
Falcon's available source data with unsupported common callbacks reported; model
snapshot-owned stage terrain independently of rendered geometry. Site decoding
must remain offline. Do not replace these libraries with new reducer/session code.
