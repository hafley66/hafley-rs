# Kneeman

V1 simulation extracted from the Kneeman recovery worktree, with its existing mechanics,
ship, items, drawing geometry, state snapshots, replay, storage, and GGRS adapter.
Godot input sampling, rendering, sprite loading, UI, and WebRTC transport remain in
the consuming `godot-shell` crate.

```rust,ignore
pub fn step(state: &SimState, inputs: &[&InputFrame], tune: &Tune) -> SimState;
pub fn net::checksum(state: &SimState) -> u128;
```

The caller retains the prior snapshot, samples inputs for one tick, and receives the next
snapshot. GGRS saves and restores those values through `net::Smash`. Game-specific input
semantics remain here; the wire packet comes from the shared `input` crate.

```sh
cargo test -p kneeman --lib
cargo test -p kneeman --features storage --lib
```

`storage` enables SQLite, `matchbox` enables the optional Matchbox transport, and `wasm`
also enables GGRS browser time support. The default package has no Godot dependency.
The recovery workspace patches GGRS locally, so its `just game-test` remains an integration
gate in addition to tests run from this workspace.

Source provenance: `kneeman-lines/6_recovery/crates/godot-shell/src/v1` and its pure
`input_frame`, `asset_wire`, and `netplay` helpers at recovery commit `018568b`.
Existing filenames and tests are preserved. The internal V1 modules still mix reusable
mechanisms with game policy; this extraction establishes game-package ownership.
