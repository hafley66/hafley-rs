# Current integration

One application, one boot scene, one simulation. Run commands from games/kneeman.
The older alternate document runtime is outside the active application and its Cargo lockfile.

| Responsibility | Owner |
| --- | --- |
| Fighter, ship, items, stage cells, replay | games/kneeman/src |
| Reducer composition and replay tools | crates/redux |
| Wire input and axis quantization | crates/input |
| GGRS simulation adapter | crates/rollback |
| RSX UI macro | crates/egui-rsx-macro |
| Godot polling, touch, bindings, menu and debugger | app/crates/godot-shell |
| Shared build/export/serve/publish | tools/godot-web |
| Game-specific profiles and web assets | app/deploy |
| Camera/gallery capture, workshop clip conversion, ZIP install | games/kneeman/tools |

General input remapping and game-independent menu extraction remain on the worklist.
The separate egui-kit library has no new integration claim. Cascade remains a test receipt.
SQLite remains an isolated lab.

## Current verification

- Existing game tests: 426 passed. Shell tests: 67 passed after retiring alternate-runtime tests.
- Art tests: capture ZIP roundtrip, roster preservation, invalid paths/missing frames, and strip/grid
  conversion through the same installer passed.
- Deployment contract tests pass, including nested poses/ URLs and profile isolation.
- Native and release WASM/Godot export pass with one Game3 export preset.
- Ad-hoc Playwright: synthetic camera capture, gallery input, ZIP download imported by the actual
  CLI, persistence after reload, mobile-viewport touch capture, rendered game at tick120,
  stable freeze, keyboard input and menu opening pass without JavaScript runtime exceptions.
- Browser testing exposed synchronous pause notification reentry into a mutably borrowed
  GDExtension node. The freeze now defers the pause call until the callback releases its borrow.
  The first-run missing identity-key lookup also now checks key existence.
- Local /rtc, /turn and /ev return404 because the local static server has no backend services.
  Emscripten emits its main-thread blocking warning. These remain visible in the browser log.
- Physical phone camera/gamepad and two-peer network acceptance remain unverified.
- dl has no destination .dl program; no lint pass claimed.

Source removals and recoverable locations are in 0_sources.md. The previous alternate-runtime
test compilation failure is no longer part of this application's test graph.

Local evidence: /private/tmp/game3-focused-tests-final.log, game3-focused-export.log,
game3-playwright-final.log and the game3-playwright-* screenshot/ZIP directories.
Ad-hoc runner: /private/tmp/0_game3_playwright.cjs. It uses an installed Playwright package
from the local machine; it is not an application build dependency.
