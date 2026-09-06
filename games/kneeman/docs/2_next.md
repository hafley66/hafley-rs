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
   Next online gates: host-tab replacement, burst loss, cross-network
   and physical phones. Current two peers ran in isolated contexts on one machine.
2. Complete semantic/physical input separation and customization: use Godot InputMap for device
   bindings, preserve the semantic tick packet, and make Controls edit/persist bindings and derive
   its displayed prompts from those bindings. Cover movement, c-stick, jump/short hop, attack,
   special, shield/dodge, grab/throw and menu for keyboard, both gamepads and mobile touch.
   Test press/hold/release, focus loss, reconnect, simultaneous inputs, remap/reload/reset and replay.
   Remaining gaps: gamepad axes/triggers and P2 remapping, pad prompt strings,
   fixed touch actions/layout and menu remapping. Reuse RawPad -> PadMemory -> InputFrame.
   Keyboard browser evidence: remap F, reload after 100 ms retains F, Escape cancels in Controls,
   reset/reload restores defaults. Web controls now use synchronous localStorage with ConfigFile
   serialization and a legacy user:// fallback; native retains ConfigFile disk writes. Injected
   Storage.setItem SecurityError shows session-only failure and preserves the prior saved F.
   Artifacts: /private/tmp/game3-input-browser-0CeMWA. Tests: 428 game + 68 shell.
   Compact Controls layout checked at 1440x1000; physical devices remain untested.
   P1 pad actions now bind to the first connected device instead of device -1 (all pads).
   Connection changes rebind those InputMap events and clear edge memory. Keyboard/touch
   bindings remain intact. Browser synthetic-pad receipt: pad 1 moves only fighter 2 upward
   on jump (410 -> 388.65); pad 0 moves only fighter 1 (410 -> 384.24); removing pad 0
   reassigns pad 1 to fighter 1. Each assertion checks the other fighter stays within 1 unit.
   Log: /private/tmp/game3-pad-browser.log; runner: /private/tmp/1_game3_pad.cjs.
   430 game + 69 shell tests pass. Jump isolation/reassignment is runtime-verified; other pad
   buttons and physical devices still need runtime coverage. A native GDScript probe failed
   to parse because joy_connection_changed is a signal there; it provides no test evidence.
   Published with the existing game3 profile; nginx validation/reload passed and the original
   /game/ pack remains unchanged. PRODUCTION=1 node /private/tmp/1_game3_pad.cjs repeats all
   three isolated-jump assertions against the live artifacts, exit 0 without browser exceptions.
   Production log: /private/tmp/game3-pad-production.log; image: /private/tmp/game3-pad-isolation.png.
   P2 grab follow-up: the raw adapter accepted Back only despite the manual and P1 bindings
   listing Y/Back. P2 now accepts both aliases; a complete button-layout regression pins
   the mapping. 430 game + 70 shell tests pass. The opt-in web debug snapshot includes fighter
   state names. Local export browser test covers Y/Back -> Grab, X -> Jab, L1 -> Shield,
   B -> SpecialN for each player, asserting the other remains Stand, plus isolated jumps
   and disconnect reassignment. Fighters are separated through controller movement first:
   the initial close-range probe timed out after a grab and is not a passing receipt.
   Command: BUTTONS=1 node /private/tmp/1_game3_pad.cjs.
   Log: /private/tmp/game3-pad-buttons-separated.log. The grab-alias export was subsequently
   published through 0dc1f1e on 2026-09-06. Before publish, /private/tmp/game3-pad-trigger.log
   additionally verified R2 -> Jab and right shoulder -> Air for both players, each with
   the other player remaining Stand. The existing game3 publish profile validated/reloaded nginx;
   deployed WASM matches the local artifact and the original /game/ pack remains unchanged.
   Production rerun passed 14 isolated button-state assertions (7 per player), isolated jumps
   and controller reassignment, exit 0 without browser exceptions. Holding each jump control
   for 45 simulation ticks produced observed shoulder/A upward excursions of 73.15/291.83
   units for P1 and 76.93/306.74 for P2. Heights are browser samples, not exact-apex or PM-parity
   measurements; the gate asserts both positive and A exceeds shoulder by more than 10 units.
   Command: PRODUCTION=1 BUTTONS=1 HEIGHTS=1 node /private/tmp/1_game3_pad.cjs.
   Log: /private/tmp/game3-pad-full-production.log. Physical devices and analog threshold
   sweeps remain untested; these are synthetic browser gamepad inputs.
3. Establish original Project M version/source receipts and its behavior ledger alongside Melee.
   Use one fighter mechanic at a time, with transition order, clocks, inputs and expected results.
   Text-reference checkpoint: docs/3_pm_baseline.md pins PM-CC revision 6e63ffa9 and exact
   archived 3.6/annotated 3.6.1 hashes. tools/0_pm_codes_receipt.mjs verifies three shared
   patch blocks. Next mechanic is jump-canceled grab; decode its patch and establish timing
   before implementing. Original release identity and Falcon character data remain unverified.
   Provisional Game3 jump-canceled grab now shares the standing-grab entry transition.
   431 game + 70 shell tests pass, including all current squat ticks/first airborne tick
   and full-state replay/restore. Local browser jump at 60 -> grab at 61 -> freeze at 62:
   Falcon Grab at y=410; other fighter Stand. Log: /private/tmp/game3-jump-grab-browser.log;
   runner: /private/tmp/1_game3_jump_grab.cjs; image: /private/tmp/game3-jump-grab.png.
   Not published. PM source interpretation, momentum/priority and run-up/held-item cases remain
   open; docs/3_pm_baseline.md distinguishes this Game3 rule from unverified PM equivalence.
   Follow-up: four run-up/held-gun/up-attack combinations now pass full-state replay over
   100 ticks each, including snapshot restore. Fighter grab retains reduced forward momentum;
   held-item throw keeps priority and excludes the owner from self-hits. 432 game + 70 shell
   tests pass; /private/tmp/game3-jc-context-tests.log. No new gameplay or deployment changes.
4. Debugger now exposes Falcon jump-grab and a generic Replay step for recorded input traces.
   Local production export: backtick opens Terrain & replay; Falcon jump-grab restores tick 60,
   Replay step reaches JumpSquat at 61 then Grab at 62, both y=410. Verify matches all 45 ticks;
   Restore returns the identical tick-60 checksum. A live Step followed by Replay step rejects
   changed state and requests restore. Restoring then stepping all 45 inputs finishes at 105;
   another step leaves tick/checksum unchanged. Screenshots: /private/tmp/game3-input-browser-MqzGl7
   (4_fixture through 10_finished). No browser exceptions; local /rtc, /turn and /ev return 404
   because this offline export server has no relay routes. 432 game + 71 shell tests pass;
   /private/tmp/game3-replay-step-tests.log. PM equivalence remains unverified.
   Export + production relay passed 549 confirmed combat frames and 180 resumed frames after
   data-channel reconnect; peer close subsequently returned offline. Runner exit 0.
   Command: LOCAL_EXPORT=1 CONFIRMED=1 COMBAT=1 RECONNECT=1 node /private/tmp/1_game3_online.cjs.
   Log: /private/tmp/game3-replay-online.log; screenshots: /private/tmp/game3-online-qpdlPh.
   Published through de86527 on 2026-09-06 using the existing game3 profile. Nginx validation
   passed; remote WASM matches local SHA-256
   47a248bd330033e2bd49a5309f23a27df928c9b6c95baf1679e41db633cbf9dd.
   Both pack hashes remain as recorded below, including the protected original /game/.
   Production smoke test also reaches Grab/Stand at tick 62, Falcon y=410 and the same checksum
   as the local debugger; exit 0 without browser exceptions.
   Command: PRODUCTION=1 node /private/tmp/1_game3_jump_grab.cjs.
   Log: /private/tmp/game3-jump-grab-production.log.
   Production guest-tab replacement also passes: 549 initial confirmed frames, close guest,
   create a fresh page in its isolated browser context, rejoin the same room, receive host
   resume/Tune, then 180 shared confirmed frames match with game ticks 757/758. Closing the
   replacement returns the host offline after timeout; runner exit 0, no browser exceptions.
   Command: CONFIRMED=1 COMBAT=1 RECONNECT=1 REPLACE_TAB=1 node /private/tmp/1_game3_online.cjs.
   Log: /private/tmp/game3-replacement-tab.log; screenshots: /private/tmp/game3-online-5JwRaI.
   Host replacement, changed device/network and physical phones remain unverified.
   Next: remaining input customization gates and burst-loss/host-replacement acceptance.
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
Initial publish through 66dd491 to /game3/ on 2026-09-06; reconnect fix republished below.
Original /game/ remains intact.
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
That export was initially verified locally; the subsequent reconnect fix is now published below.
Message-loss follow-up: local export with the production relay, 60 ms send delay,
and every tenth outgoing data-channel message dropped after the first 30 sends.
Both peers exercised loss (115/1186 and 116/1193 dropped/sent attempts); all 545
shared confirmed frames 0..544 matched. The running screenshot shows 60% damage.
Peer close reached reconnecting then offline without browser exceptions; runner exit 0.
This injects application-message loss before RTCDataChannel.send, not physical network
packet loss. Burst loss, returning-peer reconnect and cross-network behavior remain open.
Command: LOCAL_EXPORT=1 CONFIRMED=1 COMBAT=1 DELAY_MS=60 DROP_EVERY=10 node /private/tmp/1_game3_online.cjs.
Log: /private/tmp/game3-confirmed-loss.log. Screenshots: /private/tmp/game3-online-r5i2yn.
No production publish or game source change was performed for this test.
Reconnect follow-up: closing the live data channel while retaining both tabs reproduced
a guest timeout. /private/tmp/game3-reconnect-wire.log shows the host sent a 62,280-byte
resume envelope but no Tune envelope. The combined resume/Tune burst exceeds Godot's
default 65,535-byte outbound buffer; ignored send errors hid the failure.
The signaling socket now bounds both buffers at 256 KiB. Host room/resume/Tune send
failures are logged and return offline. A regression sizes serialized/base64 state plus Tune
and reserves 16 KiB for envelopes, SDP, ICE and terrain metadata. 430 game + 69 shell tests pass.
Local rebuilt export + production relay: 549 initial confirmed frames match; both tabs
reconnect and 180 resumed confirmed frames match, with game ticks advancing to 747/746.
Both resume and Tune are observed sent and received. Subsequent peer close returns offline;
runner exits 0 without browser exceptions. This covers retained-tab transport reconnection;
page reload/replacement, physical phones and cross-network behavior remain open.
Command: LOCAL_EXPORT=1 CONFIRMED=1 COMBAT=1 RECONNECT=1 node /private/tmp/1_game3_online.cjs.
Log: /private/tmp/game3-reconnect-fixed.log. Screenshots: /private/tmp/game3-online-cIM6ni.
Buffer API source: https://docs.godotengine.org/en/4.5/classes/class_websocketpeer.html.
Published the reconnect fix on 2026-09-06 with the existing dedicated game3 profile;
nginx validation/reload succeeded. Remote WASM matches the tested local artifact,
and the original /game/ pack hash remains unchanged. The successful burst is browser-verified.
Production-only rerun also passed: 549 initial and 180 resumed confirmed hashes match,
resumed ticks 748/747, disconnect timeout reaches offline, no browser exceptions, exit 0.
Log: /private/tmp/game3-reconnect-production.log. Screenshots: /private/tmp/game3-online-hJorWF.
Command: CONFIRMED=1 COMBAT=1 RECONNECT=1 node /private/tmp/1_game3_online.cjs.
Send-error follow-up on the published build: after a successful 549-frame confirmed
comparison, interrupt the data channel and report WebSocket.bufferedAmount as 256 KiB
immediately after the resume send. Exactly one Tune send fails with ERR_OUT_OF_MEMORY.
Both peers return offline and each advances more than 30 simulation ticks from tick 935;
no browser exceptions, runner exit 0. The injected bufferedAmount exists only in the
isolated browser subclass; server configuration and deployed artifacts are unchanged.
Command: CONFIRMED=1 COMBAT=1 RECONNECT=1 FAIL_RESUME_TUNE=1 node /private/tmp/1_game3_online.cjs.
Log: /private/tmp/game3-send-failure.log. Screenshots: /private/tmp/game3-online-vLu53Q.
This covers the Tune-send rejection branch. Separate room/resume rejection injection,
replacement-tab rejoin and burst-loss cases remain unverified.
Remote hashes matched the tested artifact:

- Game3 index.pck: fde60bf794d12796ff28fd1661e7afb3057337e41939f64f09c8ba157347c7df
- Game3 smash_sim.wasm: 59c52fdce74589387bc236478a35b17347134a1ab11db301eb7be08afeb79ea5
- Preserved /game/ index.pck: 5d04f53109eaf4de6b76d55c951791a0bd1a60777068f0b3bd86d7bca6bcd795

Game3 reminder runs every 30 minutes and expires 2026-09-07 00:00 EDT (epoch 1788753600).
Temporary script: /private/tmp/0_game3_overnight_boop.sh; stop marker:
/private/tmp/game3-overnight-boop.stop. It currently uses codex queue pending the Boop replacement.
Boop parent route game3-overnight owns two separately requested siblings:
feature-boop-reminders-instant (Astra high: cross-harness reminders, Instant turn widget, favorite
reasons) and feature-boop-ssh-android-research (Sol high: SSH/Tailscale/Android research).
Use boop wait --me --as game3-overnight for receipts. Their changes need review before integration.
Final sibling handoff: Boop f0a9566, 596c9f4 and receipt 09b6556 are pushed to
origin feature/boop-reminders-instant. Instant 5b8ee40 and f7a647f remain local in
/Users/chrishafley/projects/instant-worktrees/boop-reminders-instant-integration/instant.
Boop TASKS/1_reminders_instant_handoff.REPORT.md records 404 checks and exact live/skip
boundaries; Instant docs/0_boop_turn_widget.REPORT.md records native/browser receipts
and an unchanged-base attribution snapshot failure. No merge, install or reminder cutover
has occurred. Primary Boop main.rs overlap requires review before integration.
