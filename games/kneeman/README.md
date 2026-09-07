# Kneeman

Kneeman's fixed-tick simulation, ship, items, drawing geometry, state snapshots,
replay, storage, and GGRS adapter.
Godot input sampling, rendering, sprite loading, UI, and WebRTC transport live in
`app/crates/godot-shell` in this repository. Run application commands from this directory.

```sh
just game-test                 # game + Godot shell, including existing replay tests
just run                       # build native extension and launch Godot
just game3-build               # fresh production web export, no publish
just web-export-check game3    # serve/check that artifact on loopback, then stop
just game3-publish             # build and publish only /game3/
just web-dev                   # build and serve http://127.0.0.1:8787/game3/
just web-plan                  # inspect target without executing deployment
```

The application has a nested Cargo workspace so its Godot nightly, Emscripten flags,
and vendored GGRS patch do not change unrelated tools in the root workspace. All
application path dependencies resolve inside this repository. `app/godot/content`
is a relative symlink to the application's content directory.

Sprite packs remain ignored, user-supplied inputs. A fresh checkout needs
`just assets-from <existing-assets-directory>` before export. The exporter rejects
missing boot sprites. Tool locations use `GODOT45` / `EMSDK_ENV` or ignored
`app/deploy/profiles/{game3,local}.local.env`. Committed profiles contain target
configuration only. The preserved `game` profile documents the existing /game/
deployment; the normal publish recipe selects /game3/.

See [source receipts](docs/0_sources.md) and [integration status](docs/1_integration.md).
The active task list is [2_next.md](docs/2_next.md). Art capture/import commands are in
[tools/README.md](tools/README.md). There is one active simulation and boot scene.

For contact/replay inspection, press backtick to open **Terrain & replay**. **Kick ground**
and **Kick air** load 60-tick Falcon landing traces with an overlapping grounded or airborne
opponent. **Replay step** advances one recorded input; **Verify replay** checks every tick;
**Restore start** restores the snapshot. Ground contact deals 10 damage; the airborne control
takes 0. These fixtures use authored Game3 timing, with PM parity gaps recorded in the ledger.

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
The application workspace patches GGRS locally, so `just game-test` remains an integration
gate in addition to tests run from the root workspace.

Source provenance: `kneeman-lines/6_recovery/crates/godot-shell/src/v1` and its pure
`input_frame`, `asset_wire`, and `netplay` helpers at recovery commit `018568b`.
Existing filenames and tests are preserved. The internal V1 modules still mix reusable
mechanisms with game policy; this extraction establishes game-package ownership.
