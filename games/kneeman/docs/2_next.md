# Game3 worklist

Target: a playable PM-inspired fighter with the existing ship, items, stage destruction,
friend-photo characters, replay debugger and online play. One application in this repository.

## Current work

| Task | Status / acceptance |
| --- | --- |
| Single runtime | Alternate document simulation and selector scenes removed from the active app; original sources retained externally |
| Shared tools | input packet, redux, rollback, RSX macro and web driver consumed |
| Photo frames | Camera/gallery -> local ZIP -> validated assets/roster import; capture page bundled with web export |
| Workshop clips | Search/download commands recovered; local strip/grid conversion uses the same roster installer |
| Regression | Existing fixed-input/replay/rollback suites remain the game gate |

## Next, in order

1. Publish the browser-tested build to /game3/. Ad-hoc Playwright passed capture/export/import,
   saved-frame reload, mobile viewport capture, tick-120 freeze, keyboard/menu interaction and
   debugger rendering. Finish debugger pause/step/restore and two-peer input/checksum agreement.
   Physical phones and two-peer networking remain untested.
2. Reusable remapping: use Godot InputMap for device bindings, preserve the existing game input
   packet, and make the Controls UI edit/persist bindings. Cover keyboard, gamepad and touch.
3. Establish original Project M version/source receipts and its behavior ledger alongside Melee.
   Use one fighter mechanic at a time, with transition order, clocks, inputs and expected results.
4. Extend the recorded-input tests for the selected PM mechanic; expose its replay in the debugger.
5. Extract reusable menu widgets as they are exercised by controls and character import.

Art remains part of the game: gallery/camera frames, workshop PNG clips and metadata, SVG/drawn
items, reference ghosts, and character selection. Workshop GML/moveset execution, automatic
background removal, and authenticated remote photo upload are not implemented by the import tools.

Sources are indexed in 0_sources.md; current wiring is in 1_integration.md. Avoid returning to
SQLite/document/state-query architecture experiments while working through this list.
