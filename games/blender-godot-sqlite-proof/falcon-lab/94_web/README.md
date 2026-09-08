# Falcon Godot Web

From the parent Falcon lab:

```sh
just web-setup
just web-deploy
```

The sequential deployment runs native tests, offline asset baking, native editor
extension build, release WASM build, Godot export, HTTP/header checks, local browser
acceptance, backup/publish, and production browser acceptance. Individual commands:
`web-build`, `web-check`, `web-verify`, `web-publish`, `web-plan`.

Only `https://hafley.codes/game3/` is writable by this deployment. The publish gate
requires the exact artifact hashes tested in the browser. It snapshots the current
Game3 directory, uses rsync delayed updates, compares deployed artifact hashes,
and verifies every regular file under the protected `/var/www/smash-godot/`
before and after. Failure restores the prior Game3 backup. There is no nginx
configuration change or reload. The deployment uses the existing SSH access.
Backups are retained; there is no automatic retention/deletion policy.

## Reused tooling

The existing driver and validated profiles are imported from the sibling checkout
`hafley-rs-game-runtime/tools/godot-web` and its Kneeman deployment directory.
Those files were not modified. Keep that checkout alongside `hafley-rs`; this is
an explicit current development dependency, not a published deployment package.
The shared driver supplies export, profile preparation, serving, MIME/header
checks and rsync/SSH commands. Falcon adds its asset bake, application entry point,
browser gameplay receipt, and protected-target guards.

Toolchain: Godot 4.5 plus matching Web export templates, installed Emscripten,
Rust nightly-2026-05-18 and rust-src, Node/pnpm, ffmpeg, rsync and SSH.
`GODOT45` and `EMSDK_ENV` override the installed tool locations. Cargo and pnpm
lockfiles pin dependencies. Web C code uses PIC/pthreads for the Godot side module.
The shared ring crate's GGRS lab is disabled here to avoid unrelated wasm-bindgen
imports; native rollback functionality keeps its default feature.

## Runtime boundary

Brawl decoding and asset extraction run only in `falcon-web-bake`, before export.
The baked binary contains simulation actions, pose-row arrays, a deterministic
input sequence and the native presentation oracle. It is decoded once at startup.
Per-tick execution uses the existing Rust simulation, shared numeric metadata,
SQLite publication/readback, shared geometry and generated Godot payloads.
No brawllib, wgpu recorder, or ingest pipeline is linked into the browser host.

Open the site normally for keyboard and on-screen controls. `?demo=1` runs the
300-tick proof; `?inspect=1` additionally exposes generated status/row metadata
for automated testing. This diagnostic JSON transfer is off during normal play.
The proof asserts native/browser row equality, SQL readback, mesh acknowledgments,
one hit for 18 damage and 120 exact snapshot replay states. Browser tests exercise
keyboard and synthetic touch. They also create screenshots, H.264 MP4 and JSON
receipts in a fresh temporary directory.

Browser testing currently uses Chromium. Physical phones, Safari, cross-browser
determinism and online play are unverified. The threaded Godot template requires
cross-origin isolation. Emscripten warns about blocking on the main browser thread;
this build does not establish zero allocations, zero copies or optimal tick cost.
Godot itself accounts for most of the compressed download. The pose rendering
still allocates per tick; optimization requires separate profiling.

Important release behavior: calls with side effects must remain outside GDScript
`assert`, because release export strips assertions. Rust runtime assertions remain
active and enforce the receipt/replay checks.
