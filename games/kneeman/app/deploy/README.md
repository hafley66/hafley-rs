# Kneeman Godot publish profiles

Run commands from `games/kneeman`, which owns the justfile. The reusable implementation
lives at `tools/godot-web/1_web.mjs`; this directory supplies Kneeman's profiles,
web scripts, asset requirements and nginx configuration.

| Profile | Browser URL and worker scope | Export | Remote directory |
| --- | --- | --- | --- |
| `game` | `https://hafley.codes/game/` | `build/web` | `/var/www/smash-godot/` |
| `game3` | `https://hafley.codes/game3/` | `build/web-game3` | `/var/www/smash-godot-game3/` |
| `local` | `http://127.0.0.1:8787/game3/` | `build/web-local` | none |

`just web-plan game3` prints the selected profile and publish commands without
running a subprocess or making a network request. `just web-check` exercises
exports, headers, URL isolation, and push worker scope using temporary fixtures,
loopback HTTP and browser API stubs. It does not compile or launch Godot.
`just web-export-check game3` serves the real export on an ephemeral loopback
port, checks its HTML, asset responses, WASM MIME types, isolation headers and
profile, then closes the server. It never contacts the production probe URL.

The `game` profile preserves the original `/game/` target as a reference. No default
recipe publishes it.
`just game3-build` builds an independent export; `just game3-publish` builds then
publishes it. Both profiles currently export the existing `Evidence V1` boot scene.
The Rust-authored cascade receipt is test-only and is run by `just ship-cascade`.

`just web-dev` builds and serves the local export. `just web-serve` serves an
existing local export; `just web-probe` checks its HTML/base and COOP/COEP headers.
The local server binds only `127.0.0.1`; signaling services are not started here.
Browser asset URLs use an injected `<base>`. Push registration explicitly uses
the profile scope, and worker notification clicks use `self.registration.scope`.
The worker retains V1 push behavior and has no fetch cache.

Sprite packs are local inputs excluded from Git. Restore them before exporting a fresh worktree:

```sh
just assets-from /Users/chrishafley/projects/games/smash/assets
just assets-from /Users/chrishafley/projects/smashy/godot/assets
```

The first source supplies the built-in frog/zombie and mech image; the second supplies
Falcon/Lucas strips. The recipe copies image files and preserves `godot/assets/roster.json`.

Build prerequisites match V1: pinned Rust nightly and Emscripten target,
Emscripten SDK, Godot 4.5 and its web export templates. Override tool paths with
`GODOT45` and `EMSDK_ENV`, or put those keys in ignored
`app/deploy/profiles/{game,game3,local}.local.env`. Committed `.env` profiles contain
only non-secret target configuration. Overrides cannot change publish targets.
The WASM build explicitly selects the channel in `rust-toolchain.toml`, including
when the calling environment sets `RUSTUP_TOOLCHAIN`. Export first builds the
native extension for Godot's editor class scan. A failed export removes the
publish receipt; Godot `ERROR:` output is treated as failure even if it exits 0.

Before the first `/game3/` publication, the operator must separately install
`nginx/1_game3.conf` into the existing TLS server's includes and create its remote
directory. This task does not install or reload server configuration. The
existing V1 `/game/` nginx file and remote deployment remain untouched.
Publishing uses the existing rsync then nginx-test/reload sequence. It verifies
that the exported `profile.json` exactly matches the selected target.

Source receipts: V1 `justfile::{godot-wasm,godot-export,godot-deploy}`,
`deploy/nginx/godot.conf`, `deploy/web/{push,sw,turn-probe}.js`,
`e2e/tests/constants.ts::GAME_PATH`, and `export_presets.cfg`. The V1 browser
scripts are retained here with profile-specific registration and click handling.
