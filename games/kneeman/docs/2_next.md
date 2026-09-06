# Game3 worklist

Target: a playable PM-inspired fighter with the existing ship, items, stage destruction,
friend-photo characters, replay debugger and online play. One application in this repository.

Rule boundary: items may change abilities/contact/physics, and teleporting into another game may
select different rules. Author these choices as data consumed by reusable simulation code. Rule
selection and transitions must survive snapshots/replay/rollback. Extend the existing rule data
when a concrete gameplay case requires it; avoid a separate document/parser architecture.

## Current work

| Task | Status / acceptance |
| --- | --- |
| Single runtime | Alternate document simulation and selector scenes removed from the active app; original sources retained externally |
| Shared tools | input packet, redux, rollback, RSX macro and web driver consumed |
| Photo frames | Camera/gallery -> local ZIP -> validated assets/roster import; capture page bundled with web export |
| Workshop clips | Search/download commands recovered; local strip/grid conversion uses the same roster installer |
| Regression | Existing fixed-input/replay/rollback suites remain the game gate |
| Falcon | Fresh sessions select Falcon/Lucas; built-in art-to-kit mapping corrected; existing movement/attack strips wired. Commit 47ea707 |
| Destruction fixture | Shared simulation/debugger setup; 240 input ticks break cells and replay with matching checksums. 428 game + 67 shell tests pass |
| Keyboard bindings | Live InputMap editing for movement, c-stick and gameplay buttons; ConfigFile persistence/reset, side-aware labels, malformed-key validation and focus-loss clearing. 428 game + 68 shell tests pass |
| Debugger browser receipt | Fixture tick 1, Step tick 2, Capture/Verify matched all 283 recorded ticks, Restore returned tick 2 and the identical checksum. Artifacts: /private/tmp/game3-input-browser-5pK5sw |
| Production netplay | Two isolated Chromium contexts joined a unique private room through production signaling/WebRTC. Two runs matched 180 and 181 same-tick snapshot hashes with scripted movement; peer close -> reconnecting -> offline after timeout, no browser exceptions |

## Next, in order

1. Complete online acceptance for the published /game3/ build. Ad-hoc Playwright passed capture/export/import,
   saved-frame reload, mobile viewport capture, tick-120 freeze, keyboard/menu interaction and
   debugger rendering, fixture pause/step/capture/verify/restore and two-peer movement/checksum agreement.
   Next online gates: returning-peer reconnect, packet loss, cross-network
   and physical phones. Current two peers ran in isolated contexts on one machine.
2. Complete semantic/physical input separation and customization: use Godot InputMap for device
   bindings, preserve the semantic tick packet, and make Controls edit/persist bindings and derive
   its displayed prompts from those bindings. Cover movement, c-stick, jump/short hop, attack,
   special, shield/dodge, grab/throw and menu for keyboard, both gamepads and mobile touch.
   Test press/hold/release, focus loss, reconnect, simultaneous inputs, remap/reload/reset and replay.
   Remaining gaps: gamepad axes/triggers and P2 device isolation/remapping, pad prompt strings,
   fixed touch actions/layout and menu remapping. Reuse RawPad -> PadMemory -> InputFrame.
   Keyboard browser evidence: remap F, reload after 100 ms retains F, Escape cancels in Controls,
   reset/reload restores defaults. Web controls now use synchronous localStorage with ConfigFile
   serialization and a legacy user:// fallback; native retains ConfigFile disk writes. Injected
   Storage.setItem SecurityError shows session-only failure and preserves the prior saved F.
   Artifacts: /private/tmp/game3-input-browser-0CeMWA. Tests: 428 game + 68 shell.
   Compact Controls layout checked at 1440x1000; physical devices remain untested.
3. Establish original Project M version/source receipts and its behavior ledger alongside Melee.
   Use one fighter mechanic at a time, with transition order, clocks, inputs and expected results.
4. Extend the recorded-input tests for the selected PM mechanic; expose its replay in the debugger.
5. Extract reusable menu widgets as they are exercised by controls and character import.

Art remains part of the game: gallery/camera frames, workshop PNG clips and metadata, SVG/drawn
items, reference ghosts, and character selection. Workshop GML/moveset execution, automatic
background removal, and authenticated remote photo upload are not implemented by the import tools.

Sources are indexed in 0_sources.md; current wiring is in 1_integration.md. Avoid returning to
SQLite/document/state-query architecture experiments while working through this list.

## Active continuation

User requested continuous iteration, small code growth, frequent scoped commits/pushes, and
resource restraint. Use CARGO_BUILD_JOBS=1, one browser run, and no redundant concurrent builds.
Latest local browser artifacts: /private/tmp/game3-playwright-fyAxQi (Falcon/Lucas visible).
Published through 66dd491 to /game3/ on 2026-09-06. Original /game/ remains intact.
Production browser reached offline simulation and Controls, then resumed the game. Artifacts:
/private/tmp/game3-input-browser-e91D3j. HTTP GET /rtc returns 400 with "Upgrade header did not
include websocket"; GET /status is 404. The reference relay source exposes /status, but the
public route is unavailable. Leave server routing unchanged pending a scoped deployment fix.
WebSocket signaling succeeded independently. Ad-hoc test /private/tmp/1_game3_online.cjs used
two contexts, unique private room, 900-tick scripts with opposite movement, same-tick snapshot
hash comparison, then peer close and the 12-second reconnect timeout. Receipts:
/private/tmp/game3-online-check.log (180 matches), /private/tmp/game3-online-disconnect.log
(181 matches and offline recovery), /private/tmp/game3-online-ZTZfCn (screenshots).
No simulation changes or compiler jobs were needed for this acceptance run.
Changing-input follow-up: /private/tmp/game3-online-combat.log and
/private/tmp/game3-online-Mu6Lzj matched 550 same-tick hashes through movement, attack/held,
jump/held, special and shield inputs, followed by reconnect timeout -> offline. Final screenshot
shows both fighters at 0%: this is input-transition parity evidence, not a proven hit exchange.
Run with COMBAT=1 node /private/tmp/1_game3_online.cjs. Consecutive identical input frames are
encoded as duration runs; the initial per-tick JSON exceeded the browser URL limit before boot.
Next combat test must assert damage/hit state, then compare its network snapshots.
Hit fixture follow-up: `replay_tests::falcon_walk_in_hits_and_replays_every_tick` lands two
Falcons, walks at +/-32/127 during ticks 60..84, then attacks every 30 ticks from tick 90.
Every input passes encode/decode; 360 ticks produce peak total damage 60 and byte-identical
replay, including a mid-run snapshot restore. Full gate: 429 game + 68 shell tests pass.
The earlier +/-0.25 scripts quantized to +/-31/127, below the movement threshold. Their peer
hash agreement does not prove movement. The wire-representable fixture fixes this test input.
Production /private/tmp/game3-online-wire-hits.log and /private/tmp/game3-online-mpf1MB show
60% damage, but strict historical hash comparison failed at ticks 93, 214, 273, 274.
`webtest::refresh` records displayed/predicted snapshots once per physics frame; rollback
catch-up does not replace every historical entry. Diagnose with confirmed snapshots or
re-simulation before calling these mismatches either resolved predictions or actual desyncs.
Keep this combat gate open. No production code was changed for the fixture.
Confirmed-state follow-up: shared rollback `Game::handle_observed` reports saved-frame hashes,
including replacements during re-simulation. Netplay keeps an opt-in 600-entry map and exposes
only frames confirmed after the rollback requests were handled; browser `confirmedAt` is
separate from displayed/predicted `at`. A save/load/re-advance regression proves replacement.
430 game + 68 shell tests pass. Local production export with the existing production relay:
- /private/tmp/game3-confirmed-browser.log: 549 confirmed hashes match; 60% damage screenshot.
- /private/tmp/game3-confirmed-delay.log: 60 ms delayed data-channel sends, 34 displayed-history
  mismatches but all 546 compared confirmed hashes match; disconnect -> reconnect timeout -> offline.
- Screenshots: /private/tmp/game3-online-Ffb7A9 and /private/tmp/game3-online-1bxM30.
This reproduces the historical-comparison failure as prediction evidence and closes that
specific gate. These tests do not establish PM parity or behavior under packet loss.
Command: LOCAL_EXPORT=1 CONFIRMED=1 COMBAT=1 DELAY_MS=60 node /private/tmp/1_game3_online.cjs.
The updated export is locally verified; it has not been published yet.
Remote hashes matched the tested artifact:

- Game3 index.pck: fde60bf794d12796ff28fd1661e7afb3057337e41939f64f09c8ba157347c7df
- Game3 smash_sim.wasm: 56abd0712209504321736da2d6cd64a992fd332b1ace1b14934c1742e519fc66
- Preserved /game/ index.pck: 5d04f53109eaf4de6b76d55c951791a0bd1a60777068f0b3bd86d7bca6bcd795

Game3 reminder runs every 30 minutes and expires 2026-09-07 00:00 EDT (epoch 1788753600).
Temporary script: /private/tmp/0_game3_overnight_boop.sh; stop marker:
/private/tmp/game3-overnight-boop.stop. It currently uses codex queue pending the Boop replacement.
Boop parent route game3-overnight owns two separately requested siblings:
feature-boop-reminders-instant (Astra high: cross-harness reminders, Instant turn widget, favorite
reasons) and feature-boop-ssh-android-research (Sol high: SSH/Tailscale/Android research).
Use boop wait --me --as game3-overnight for receipts. Their changes need review before integration.
