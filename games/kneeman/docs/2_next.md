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
| Falcon Dive | Published through 5e0eacd: grounded startup fix, catch/whiff debugger fixtures and optional-fire-art guard. Production replay and online Dive acceptance pass |
| Special ship carry | Published through 83e3456: carry correction plus Ship Dive debugger fixture. Native and production browser checks prove startup follows the moving hull, then launches and replays |
| Destruction fixture | Shared simulation/debugger setup; 240 input ticks break cells and replay with matching checksums. 428 game + 67 shell tests pass |
| Cell item/contact sequence | Published through f9a486d: Cell replay uses the same 240-tick wire-input sequence as the native test. Local and production pickup/throw/contact, restore/EOF and changed-state rejection pass |
| Keyboard bindings | Live InputMap editing for movement, c-stick and gameplay buttons; ConfigFile persistence/reset, side-aware labels, malformed-key validation and focus-loss clearing. 428 game + 68 shell tests pass |
| Debugger browser receipt | Fixture tick 1, Step tick 2, Capture/Verify matched all 283 recorded ticks, Restore returned tick 2 and the identical checksum. Artifacts: /private/tmp/game3-input-browser-5pK5sw |
| Production netplay | Two isolated Chromium contexts joined a unique private room through production signaling/WebRTC. Two runs matched 180 and 181 same-tick snapshot hashes with scripted movement; peer close -> reconnecting -> offline after timeout, no browser exceptions |
| Pad menu bindings | Published through 7c1bd9f: six actions per pad reuse capture/save/reset; production navigation, hold/release/disconnect, pause precedence and resumed neutral input verified below |
| Touch cancellation | Published through 2647139: hidden gestures cancelled, shared touch actions retained until last lift. Production touch/replay and online loss/reconnect gates pass |

## Next, in order

1. Complete online acceptance for the published /game3/ build. Ad-hoc Playwright passed capture/export/import,
   saved-frame reload, mobile viewport capture, tick-120 freeze, keyboard/menu interaction and
   debugger rendering, fixture pause/step/capture/verify/restore and two-peer movement/checksum agreement.
   Production burst-loss checkpoint on 2026-09-06 passed with one Chromium and two isolated
   contexts in a unique private room. Each outgoing data-channel message is delayed 60 ms;
   after the first 30 sends, drop messages where send_count % 120 < BURST_LENGTH. Counts
   refer to application messages, not IP packets or a measured network outage duration.
   BURST_LENGTH=8: 546 initial confirmed frames and 180 resumed frames matched; each peer
   dropped 80 messages, maximum consecutive run 8, from 1209/1214 sends before reconnect.
   BURST_LENGTH=24: 549 initial confirmed frames and 181 resumed frames matched; each peer
   dropped 240 messages, maximum consecutive run 24, from 1235/1232 sends before reconnect.
   Both tests preserved fighter handles/characters across forced channel reconnect, advanced
   beyond the interrupted clock, then reached offline after peer closure; exit 0 and no
   browser exceptions. Production runtime is the existing pad-remapping publication; no deploy.
   Commands: CONFIRMED=1 COMBAT=1 RECONNECT=1 DELAY_MS=60 BURST_LENGTH=8 (then 24)
   node /private/tmp/1_game3_online.cjs. Logs: /private/tmp/game3-burst8-production.log and
   /private/tmp/game3-burst24-production.log. Screenshots: /private/tmp/game3-online-TZJF6r
   (8-message bursts) and /private/tmp/game3-online-3foHwZ (24-message bursts).
   The initial sandboxed browser launch failed before navigation; only the completed subsequent
   runs supply acceptance evidence. Forced channel closure still logs ERR_UNCONFIGURED from
   sends against non-open channels: 5 lines in the 8-message run, 15 in the 24-message run.
   Follow-up: RtcSocket::send_to and MeshSocket::send_to now return before serialization
   when the channel is not OPEN. 432 game + 73 shell tests pass, and the rebuilt export
   passes the 24-message/60 ms test: 547 initial and 180 resumed confirmed frames match,
   226/227 messages dropped, maximum consecutive run 24 for both peers. Fighter slots and
   characters survive reconnect; peer closure reaches offline. The new console assertion
   finds zero ERR_UNCONFIGURED lines, with no browser exceptions; exit 0.
   Command: LOCAL_EXPORT=1 CONFIRMED=1 COMBAT=1 RECONNECT=1 DELAY_MS=60 BURST_LENGTH=24
   NO_CLOSED_SEND_ERRORS=1 node /private/tmp/1_game3_online.cjs.
   Logs: /private/tmp/game3-channel-guard-tests.log, game3-channel-guard-build.log and
   game3-channel-guard-browser.log. Runtime browser coverage is two-player; the matching
   mesh guard compiles but has no new three-player browser receipt. Cross-network and
   physical phones remain open. Local screenshots: /private/tmp/game3-online-vQQXAt.
   Published through 53c1ec5 on 2026-09-06; nginx validation/reload passed. Remote WASM
   matches the tested artifact: 77f7ab85dbad13c7fe44861978b687965cae41970667e2818da6f58fd7ce6991.
   Both Game3 and original /game/ pack hashes remain unchanged. Production rerun (same
   command without LOCAL_EXPORT) passes 542 initial and 180 resumed confirmed frames,
   with 240/1225 messages dropped per peer and maximum consecutive loss 24. Fighter slots
   and characters remain stable; peer closure reaches offline; no ERR_UNCONFIGURED or
   browser exceptions, exit 0. Log: /private/tmp/game3-channel-guard-production.log;
   screenshots: /private/tmp/game3-online-ccyvLA. Next: movement/c-stick input customization.
2. Complete semantic/physical input separation and customization: use Godot InputMap for device
   bindings, preserve the semantic tick packet, and make Controls edit/persist bindings and derive
   its displayed prompts from those bindings. Cover movement, c-stick, jump/short hop, attack,
   special, shield/dodge, grab/throw and menu for keyboard, both gamepads and mobile touch.
   Test press/hold/release, focus loss, reconnect, simultaneous inputs, remap/reload/reset and replay.
   Remaining gaps: native keyboard menu-navigation bindings, fixed touch actions/layout and physical devices.
   Reuse RawPad -> PadMemory -> InputFrame.
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
   Local gamepad remapping now uses device-scoped InputMap actions for both players' jump,
   short hop, attack, shield, grab and special. R2 attack is a default motion event instead
   of a separate raw-trigger branch. Controls captures a button or signed axis from the
   selected player's device, labels the live binding, persists validated integer triples
   alongside keyboard settings, and resets pads independently. Semantic InputFrame is unchanged.
   432 game + 73 shell tests pass; /private/tmp/game3-pad-remap-final-tests.log.
   Local export defaults: 14 isolated button-state assertions plus jump isolation/reassignment
   pass, exit 0; BUTTONS=1 node /private/tmp/1_game3_pad.cjs,
   /private/tmp/game3-pad-remap-defaults.log. Interactive remap screenshots:
   /private/tmp/game3-input-browser-AjkkFA. Wrong-device capture stays pending; P1 jump ->
   Godot button 7 and P2 jump -> button 8 save separately, drive only their own fighter,
   survive reload, and remove A's jump behavior. P2 shield -> positive axis 4 saves and
   reaches Shield while P1 stays Stand. Reset clears pad overrides; injected storage denial
   reports session-only save failure and preserves the prior saved defaults. After reload,
   P2 A jumps again. No browser exceptions; offline /rtc, /turn and /ev 404s are expected here.
   Final pad-cell contrast verified at 1440x1000. Escape cancels pad capture and leaves
   settings unchanged; axis 0.5 stays pending while 0.8 captures the signed axis. Capture
   starts at tick 138; remapped P1 button 7 and P2 positive axis 2 produce jumps, and Verify
   matches all 68 input ticks. Restore + 68 Replay steps returns tick 206 and identical
   checksum 90542123335d966b9b418e45386e5abfb3689e76007c8fb380a4b252000091d4;
   another Replay step leaves it unchanged. Screenshots: /private/tmp/game3-input-browser-EFYSMg,
   1_controls through 8_replayed. No browser exceptions. These samples cover capture threshold
   behavior; a complete analog gameplay threshold sweep remains open.
   Online regression on this final export passes: 549 initial and 180 resumed confirmed frames,
   ticks 747/746, preserved handles/characters and offline timeout recovery, exit 0.
   Command: LOCAL_EXPORT=1 CONFIRMED=1 COMBAT=1 RECONNECT=1 node /private/tmp/1_game3_online.cjs.
   Log: /private/tmp/game3-pad-remap-online.log; screenshots: /private/tmp/game3-online-PJG0C7.
   Published through 8034799 on 2026-09-06 using the dedicated game3 profile; nginx validation
   passed. Remote WASM matches tested SHA-256
   81fbc0d9c28cc6a8eb864d833cace334d29f7ab42a2c9706856e9446e2dade4c;
   Game3 pack and protected original /game/ pack remain unchanged.
   Production default-controls regression passes 14 isolated button assertions and 3 jump/
   reassignment assertions, exit 0 without browser exceptions.
   Command: PRODUCTION=1 BUTTONS=1 node /private/tmp/1_game3_pad.cjs.
   Log: /private/tmp/game3-pad-remap-production.log; screenshot: /private/tmp/game3-pad-isolation.png.
   To remap: Controls -> click the player's Pad label -> press the desired button/axis.
   Movement/c-stick remapping, mobile layout/menu controls and physical
   device checks remain open.
   Local stick-remapping follow-up: eight signed directions per pad now have separate
   InputMap actions, using raw action strength and the existing movement/radial c-stick
   deadzones. Keyboard priority, D-pad-over-stick priority, touch fallback and semantic
   packets are unchanged. Controls -> Gamepad movement / c-stick edits either player's
   direction using the existing button/axis capture, persistence and Reset pads path.
   432 game + 73 shell tests pass; /private/tmp/game3-stick-remap-tests.log.
   Export passes; /private/tmp/game3-stick-remap-build.log. Opt-in web snapshots expose x
   positions for directional acceptance. Code delta before this ledger: +50/-17 lines.
   Interactive evidence: /private/tmp/game3-input-browser-xolzZV, screenshots 1_controls
   through 5_movement. P1 right -> Godot button 7 ignores wrong-device input during capture,
   persists, produces Dash/Stand, survives reload and disables the old positive axis 0.
   Automated local browser run: 12 movement assertions cover both players at 0.1/0.3/0.8,
   keyboard and D-pad priority, saved button/negative-axis remaps and old-binding removal;
   every assertion checks the other fighter's x stays unchanged. Both players' saved
   c-stick-up -> button 8 reaches Usmash while the other remains Stand, and old axis 3-
   no longer triggers the smash. Browser state observations do not establish PM thresholds.
   Command: node /private/tmp/2_game3_sticks.cjs. Log: /private/tmp/game3-stick-remap-browser.log;
   screenshots: /private/tmp/game3-sticks-SmrUoN. Exit 0 without browser exceptions. The first
   runner attempt failed on its own pre-boot __smash lookup; it supplies no game evidence.
   Replay follow-up reruns those assertions, then records remapped P1 button movement,
   P2 signed-axis movement and a remapped c-stick smash. Restore returns tick 138;
   50 acknowledged Replay steps reach tick 188 and the captured checksum
   b54eaedf99857ee926185d313b8addfa550b1ecf1e1929f976ab3d92334e3695.
   An additional EOF step leaves tick/checksum unchanged. Exit 0; log:
   /private/tmp/game3-stick-remap-replay-stepped.log, screenshots:
   /private/tmp/game3-sticks-naWPRA. The preceding fast-click runner stopped one tick short;
   its Verify screenshot matched all 50 ticks, but step-through acceptance required the
   corrected runner to wait for each requested tick before issuing the next click.
   Final default-pad regression passes 14 isolated button assertions and 3 jump/reassignment
   assertions, exit 0 without browser exceptions. Command: BUTTONS=1 node
   /private/tmp/1_game3_pad.cjs; log: /private/tmp/game3-stick-remap-defaults.log.
   Final Controls/reset browser receipt: /private/tmp/game3-input-browser-ecDEqp.
   The collapsed and expanded section render at 1440x1000; scrolling exposes all eight
   directions per player (6_scroll.png). Live P2 right -> positive axis 3 and P1 left ->
   button 7 save independently. Keyboard jump -> F adds a separate keys entry. Reset pads
   removes both pad overrides, restores the labels and preserves jump=PackedInt32Array(70,0).
   Reload retains that result; default P1 axis 0 at -0.3 moves x 480 -> 467.40002 while P2
   stays at 720; P2 axis 0 at +0.3 moves x 720 -> 731.55 while P1 stays at 417.91006.
   No browser exceptions; local relay-route 404s are expected. These are synthetic pads.
   Final export online gate passes 547 initial and 180 resumed confirmed frames with 60 ms
   delay and 24-message bursts; each peer drops 240 messages from 1240/1241 sends. Fighter
   slots/characters persist across reconnect, peer closure reaches offline, and the console
   guard sees no ERR_UNCONFIGURED; no browser exceptions, exit 0. Command:
   LOCAL_EXPORT=1 CONFIRMED=1 COMBAT=1 RECONNECT=1 DELAY_MS=60 BURST_LENGTH=24
   NO_CLOSED_SEND_ERRORS=1 node /private/tmp/1_game3_online.cjs.
   Log: /private/tmp/game3-stick-remap-online.log; screenshots: /private/tmp/game3-online-ZCP8UU.
   Published through 23304b3 on 2026-09-06 via the existing game3 profile; nginx validation/
   reload passed. Remote WASM matches tested SHA-256
   bab4ac9199b5e23b3378a1f1f8a179d93ecaf84e613749a9a8be45eb91f8ceb3;
   Game3 and original /game/ pack hashes remain unchanged. Production rerun passes all 12
   movement assertions and both isolated remapped up-smashes. Captured remapped movement
   and c-stick inputs restore tick 138, then replay 50 steps to tick 188 and the same captured
   checksum 09453134205a017b76e74920e205d55fb75ce035168cd6e3ed29b3ca9bb13127.
   Extra EOF step changes neither tick nor checksum. Local and production captures use
   live browser timing; this asserts each recording against its own replay, not identical
   input timing between runs. Exit 0 without browser exceptions.
   Command: PRODUCTION=1 node /private/tmp/2_game3_sticks.cjs.
   Log: /private/tmp/game3-stick-remap-production.log; screenshots: /private/tmp/game3-sticks-qybkwc.
   Next: D-pad/menu/touch customization and physical-device checks. Movement and c-stick
   remapping is available under Controls -> Gamepad movement / c-stick.
   Local D-pad follow-up: four digital movement overrides per pad now use device-scoped
   InputMap actions and the existing capture/save/reset path. Default button priority is
   preserved; mapped axes activate digitally at the action's 0.5 threshold. Opposite D-pad
   directions cancel, allowing the stick fallback. The Controls direction section includes
   these rows and Fast-fall points to editable directions. Runtime delta: +31/-20 lines.
   Final unit/export gates pass: 432 game + 73 shell tests; logs
   /private/tmp/game3-dpad-remap-final-tests.log and game3-dpad-remap-final-build.log.
   Before the final hint-only edit, node /private/tmp/2_game3_sticks.cjs passes 20 horizontal
   movement assertions: both players, deadzones, keyboard/D-pad priority, opposing D-pad
   cancellation, button/signed-axis remaps, reload, old-binding removal and device isolation.
   D-pad-axis samples 0.3 and 0.8 reject/activate respectively. Both isolated remapped c-stick
   smashes pass. A recording containing remapped D-pad inputs restores tick 138 and replays
   51 steps to tick 189 with the captured checksum
   56d73e638756b4be56f5f6315b670e24881962b8224696af3c7e97e3a47308d5;
   EOF is unchanged. Exit 0; /private/tmp/game3-dpad-remap-browser.log,
   screenshots /private/tmp/game3-sticks-V8sbYK. These thresholds describe Game3, not PM parity.
   Final export UI receipt: /private/tmp/game3-input-browser-eCWocm. Scrolling exposes all
   12 direction rows per player. P1 D-pad-up -> button 7 and P2 D-pad-down -> button 8 save
   independently. Remapped P1 jump moves y 410 -> 358.9 while P2 stays 410; remapped P2 drop
   moves y 410 -> 577.36993 while P1 stays 410. Reset pads clears both overrides; reload
   preserves the reset. Default D-pad-up repeats P1's isolated jump; default D-pad-down
   moves P2 y 410 -> 482.16995 while P1 stays 410. No browser exceptions; local relay-route
   404s are expected. Synthetic pads only. Final default-button gate passes 14 isolated
   button assertions and 3 jump/reassignment assertions, exit 0;
   BUTTONS=1 node /private/tmp/1_game3_pad.cjs, /private/tmp/game3-dpad-defaults.log.
   The release online gate matches 547 initial and 180 resumed confirmed frames but fails
   its zero-ERR_UNCONFIGURED assertion with four messages on forced closure; publication
   paused. Log: /private/tmp/game3-dpad-online.log. Both send paths were already guarded.
   Targeted reproduction on the still-published artifact closes after a message is queued
   and delays Godot's onclose callback by 500 ms. It emits six errors explicitly attributed
   to get_packet (modules/webrtc/webrtc_data_channel_js.cpp:106), then recovers 180 matching
   frames after 549 initial matches. The error assertion fails, exit 1. Command:
   CONFIRMED=1 COMBAT=1 RECONNECT=1 QUEUED_CLOSE=1 NO_CLOSED_SEND_ERRORS=1
   node /private/tmp/1_game3_online.cjs. Log: /private/tmp/game3-queued-close-before.log;
   screenshots: /private/tmp/game3-online-kEuOiq.
   [Godot 4.5 source](https://github.com/godotengine/godot/blob/4.5/modules/webrtc/webrtc_data_channel_js.cpp)
   exposes queue_count independently and rejects get_packet on non-open channels. Pair and
   mesh receive loops now check OPEN before each read. 432 game + 73 shell tests and export
   pass; /private/tmp/game3-rtc-receive-tests.log and game3-rtc-receive-build.log.
   First guarded run had zero channel errors and matched 543 initial/180 resumed frames,
   but failed on a null-function exception from the injector's delayed callback after Godot
   teardown. Log: /private/tmp/game3-queued-close-fixed.log. The injector now cancels the
   timer when Godot clears onclose, matching its callback teardown contract.
   Corrected run passes 549 initial and 180 resumed confirmed frames, exactly one injected
   queued-message closure, preserved fighter slots/characters, offline timeout recovery,
   zero ERR_UNCONFIGURED and zero browser exceptions; exit 0. It also uses 60 ms delay and
   24-message bursts. Command: LOCAL_EXPORT=1 CONFIRMED=1 COMBAT=1 RECONNECT=1 QUEUED_CLOSE=1
   DELAY_MS=60 BURST_LENGTH=24 NO_CLOSED_SEND_ERRORS=1 node /private/tmp/1_game3_online.cjs.
   Log: /private/tmp/game3-queued-close-fixed-cancelled.log; screenshots:
   /private/tmp/game3-online-XOiPO9. Pair runtime verified; mesh receive guard compiles but
   no new three-player runtime receipt exists. Published through 425f293 on 2026-09-06 via
   the existing game3 profile; nginx validation/reload passed. Remote WASM matches tested
   SHA-256 b57fd431df336e197c9e1eeb0b16720cccdd579db15cc9990f637b0c233fbcb4;
   Game3 and original /game/ pack hashes remain unchanged. Production queued-close gate
   (same command without LOCAL_EXPORT) passes 547 initial and 180 resumed confirmed frames,
   fighter-slot/character continuity and offline timeout. Exactly one queued-message close
   is injected; no ERR_UNCONFIGURED or browser exceptions, exit 0. Log:
   /private/tmp/game3-queued-close-production.log; screenshots: /private/tmp/game3-online-DeE3bd.
   Production direction/replay gate passes all 20 horizontal movement assertions, both
   remapped c-stick smashes and exact replay of the recorded D-pad/stick inputs: restore
   tick 137, replay 48 steps to tick 185 with captured checksum
   64f8356e62a1b77ccb289a5ee54bb39882a7fea6f7191dc631decde11e690d48;
   EOF leaves tick/checksum unchanged. Exit 0 without browser exceptions. Command:
   PRODUCTION=1 node /private/tmp/2_game3_sticks.cjs.
   Log: /private/tmp/game3-dpad-production.log; screenshots: /private/tmp/game3-sticks-y7FlIC.
   D-pad remapping is published in Controls -> Gamepad movement / c-stick. Next: menu/touch
   customization; cross-network/physical-device and three-player checks remain open.
   Pause/open-menu now uses editable InputMap actions for keyboard and both pads, with the
   existing capture/save/reset paths. Menu navigation remains fixed. UI pause stays outside
   InputFrame; capture/cancel suppresses navigation and mapped pad A cannot also activate a
   focused menu entry. Runtime delta: +24/-24 lines across four files.
   Interactive local-export receipt: /private/tmp/game3-input-browser-Ui7AVH. Keyboard P
   saves, backs Controls -> Menu -> gameplay, and holds without repeated transitions;
   old Escape no longer opens from gameplay. Escape cancels capture without changing settings.
   P1 pause -> A saves, backs to Menu without activating the focused Items entry, holds
   without repetition, and resumes gameplay. Removed P1 Start leaves ticks advancing
   149 -> 192; default P2 Start pauses at 195. P2 pause -> positive trigger axis 4 saves,
   survives reload and holds at tick 40. Reset pads removes both pad overrides while
   retaining keyboard P; Reset keyboard clears that entry and Escape opens after reload.
   Screenshots 2_cancel through 17_reset_escape; synthetic pads, no browser exceptions.
   Offline /rtc, /turn and /ev 404s are expected. The subsequent label-only edit displays
   Start by name. Final 432 game + 73 shell tests pass; /private/tmp/game3-pause-final-tests.log.
   Initial export failed on sandboxed Godot editor-settings writes; the approved retry
   passes: /private/tmp/game3-pause-final-build-approved.log. Physical-device, touch-layout,
   three-player and cross-network checks remain open.
   Final export online gate passes 545 initial confirmed frames and 180 resumed frames
   after exactly one queued-message closure; resumed game ticks 744/742. Both peers have
   24-message maximum loss bursts (224/1207 and 227/1210 dropped/sent), with 60 ms send
   delay. Slots/characters remain stable and peer closure reaches offline; no channel or
   browser errors, exit 0. Command: LOCAL_EXPORT=1 CONFIRMED=1 COMBAT=1 RECONNECT=1
   QUEUED_CLOSE=1 DELAY_MS=60 BURST_LENGTH=24 NO_CLOSED_SEND_ERRORS=1
   node /private/tmp/1_game3_online.cjs. Log: /private/tmp/game3-pause-online.log;
   screenshots: /private/tmp/game3-online-D9KNja.
   Final direction/replay gate also passes 20 horizontal movement assertions and both
   isolated c-stick smashes. Restore tick 137, replay 50 recorded steps to tick 187,
   checksum 4173f332821851e0379ac30ffaf5e8008c04335f5f6f711133f4d1eaed906876;
   EOF unchanged, no browser exceptions, exit 0. Command: node /private/tmp/2_game3_sticks.cjs.
   Log: /private/tmp/game3-pause-directions-replay.log; screenshots: /private/tmp/game3-sticks-Fjbef2.
   Published through 4d57d21 on 2026-09-06 with the dedicated game3 profile; nginx validation
   and reload pass. Deployed WASM matches tested SHA-256
   71c4d490b9156e70923092f297aaa4381ccc5a09661dee95e53fe307bf6fba05.
   Game3 pack and protected original /game/ pack hashes remain unchanged. Production UI
   receipt passes: keyboard P saves, resumes, old Escape leaves ticks advancing 54 -> 93,
   held P pauses once at 95. P2 trigger remap survives reload and holds at tick 53; pad
   reset preserves P, keyboard reset clears it, and Escape opens after reload. Start labels
   render for both pads. No browser exceptions, runner exit 0.
   Log: /private/tmp/game3-pause-production.log; screenshots: /private/tmp/game3-input-browser-bLKvTd.
   Next bounded input work: editable menu navigation, followed by touch action/layout
   customization. The PM move ledger and physical/cross-network/three-player acceptance
   remain incomplete; this checkpoint does not change fighter mechanics.
   Gamepad menu follow-up: six actions per pad (down/up/left/right/accept/back) use the
   existing device-scoped InputMap capture, persistence and reset path. Controls -> Gamepad
   movement / c-stick / menu exposes the additional rows. UI samples pressed edges once per
   process frame; the existing direction repeat timer tracks the mapped action and stops on
   release, disconnect, capture, pause or menu closure. InputFrame and simulation are unchanged.
   Runtime delta: +56/-77 lines across three files. Final 432 game + 73 shell tests and export
   pass: /private/tmp/game3-menu-restored-tests.log and game3-menu-restored-build.log.
   Interactive capture/reload: /private/tmp/game3-input-browser-68QxG5. Default navigation
   reaches Characters and backs to Menu. Wrong-device input leaves capture pending. P1 down
   -> button 7, P2 accept -> positive axis 4 and P2 back -> button 8 save and survive reload.
   Local navigation assertions: six remapped actions on both pads match native keyboard
   focus/routes, with old bindings removed and wrong-device inputs ignored. Axis back at
   0.3 stays neutral; 0.8 activates once through subsequent 0.7/0.9/0.8 samples. Direction
   repeats stop on release and disconnect. /private/tmp/game3-menu-navigation-final.log
   and /private/tmp/game3-menu-fA37zk record these passing assertions, then stop on a failed
   picker-fixture precondition: mouse click did not transfer keyboard focus from the rail.
   Further fixed-index picker assertions were invalid because identity.rs persists roster
   picks independently of controls. Start-state receipt proves 2 -> 1 and 1 -> 0, each once:
   /private/tmp/game3-menu-accept-start-state.log. The diagnostic edge-mask experiment was
   removed; no engine duplicate-activation defect is claimed. Corrected fixture navigates
   keyboard focus, derives one native decrement from the actual selection, restores it,
   then holds axis accept. Both pads match that single decrement through all four samples.
   Pause precedence, independent resets and unchanged fighter positions/states after 60+
   resumed ticks also pass, no browser exceptions, exit 0. Command: HOLDS_ONLY=1 node
   /private/tmp/3_game3_menu.cjs. Log: /private/tmp/game3-menu-holds-restored-baseline.log;
   screenshots: /private/tmp/game3-menu-TP99BR. Screenshot comparisons permit one RGB level
   per channel (measured glyph rasterization variation), with no positional tolerance.
   Final export online gate passes 546 initial and 180 resumed confirmed frames, game
   ticks 756/754, stable fighter slots/characters and offline recovery after peer closure.
   Each peer drops 240 messages from 1249/1248 sends, maximum burst 24; sends are delayed
   60 ms and exactly one queued-message close is injected. Zero ERR_UNCONFIGURED and
   browser exceptions, exit 0. Command: LOCAL_EXPORT=1 CONFIRMED=1 COMBAT=1 RECONNECT=1
   QUEUED_CLOSE=1 DELAY_MS=60 BURST_LENGTH=24 NO_CLOSED_SEND_ERRORS=1
   node /private/tmp/1_game3_online.cjs. Log: /private/tmp/game3-menu-online.log;
   screenshots: /private/tmp/game3-online-xbshKj.
   Published through 7c1bd9f on 2026-09-06 using the existing dedicated profile; nginx
   validation/reload passes. Remote WASM matches tested SHA-256
   4f80e0aadf8ae0aaa45d9c734a2c23b49b4c1059f33a16f6c5c4bf04fd669f7c.
   Both Game3 and protected original /game/ pack hashes remain unchanged. Full production
   menu run passes all 12 action comparisons, old-binding removal/device isolation,
   both axis-back thresholds/holds, direction repeat/release/disconnect, one-step character
   selection through held axis accept, pause precedence, independent resets and neutral
   resumed gameplay. Exit 0, no browser exceptions. Command: PRODUCTION=1 node
   /private/tmp/3_game3_menu.cjs. Log: /private/tmp/game3-menu-production.log;
   screenshots: /private/tmp/game3-menu-gEL7Cz. Physical pads remain untested.
   Touch follow-up: reproduced hidden left-stick capture with a connected controller:
   fighter X changed from [480,720] to [620.14,720]. Log:
   /private/tmp/game3-touch-hidden-before.log; screenshots: /private/tmp/game3-touch-WRnFlc.
   Hidden layout now gates input capture. release_touch drains owned button actions and
   clears both sticks/finger IDs; layout/availability updates run before input sampling,
   including paused frames. Focus notifications reuse this cleanup. Runtime delta: +22/-11
   across three shell files; no simulation or packet changes.
   432 game + 73 shell tests pass, including Falcon/items/terrain/replay regressions.
   Export passes after retrying the sandbox-denied Godot editor-settings write.
   Logs: /private/tmp/game3-touch-tests.log and /private/tmp/game3-touch-build.log.
   One touch-enabled Chromium with a synthetic controller passes hidden input neutrality,
   visible P1-only movement, cancellation on connect/menu, stale-finger rejection after
   disconnect/resume, and held guard release on connection and simulated window blur.
   Recorded touch movement/attack/c-stick replays 72 frames from tick 133 to 205 with
   matching restored/start and replay/end checksums; stepping at EOF preserves the result.
   Command: node /private/tmp/4_game3_touch.cjs. Log: /private/tmp/game3-touch-replay.log;
   screenshots: /private/tmp/game3-touch-46jTcF. Physical touch devices remain untested.
   Same export online gate passes 547 initial and 180 resumed confirmed frames, ticks
   755/757, stable slots/characters, and offline recovery after peer closure. Each peer
   drops 240 messages from 1246/1243 sends, maximum burst 24 with 60 ms send delay.
   Exactly one queued-close injection; zero closed-send errors/browser exceptions; exit 0.
   Command: LOCAL_EXPORT=1 CONFIRMED=1 COMBAT=1 RECONNECT=1 QUEUED_CLOSE=1 DELAY_MS=60
   BURST_LENGTH=24 NO_CLOSED_SEND_ERRORS=1 node /private/tmp/1_game3_online.cjs.
   Log: /private/tmp/game3-touch-online.log; screenshots: /private/tmp/game3-online-XHGVrr.
   Next bounded task: touch action/layout customization through the existing settings path.
   Multi-finger follow-up reproduced premature guard release: two fingers hold the guard
   wedge, lifting one changed Shield to Stand while the other remained down. Browser log:
   /private/tmp/game3-touch-shared-before.log; screenshots: /private/tmp/game3-touch-RPAz1B.
   The release branch now checks remaining finger owners before releasing each action.
   Runtime delta is +8/-7 in KneeMan::input, with no new types or simulation changes.
   432 game + 73 shell tests and the web export pass. Both two-finger lift orders retain
   Shield until the last lift, then return to Stand. Prior hidden-input/menu/controller/
   simulated-blur checks also pass. Touch replay restores tick 133, verifies all 73 recorded
   ticks through 206, and matches the final checksum and EOF behavior. No browser exceptions.
   Command: node /private/tmp/4_game3_touch.cjs. Logs: /private/tmp/game3-touch-shared-tests.log,
   game3-touch-shared-build.log and game3-touch-shared-fixed.log. Screenshots:
   /private/tmp/game3-touch-nxunLK. Physical-device focus/rotation and shared ownership
   across touch versus keyboard/pad sources remain unverified. Publication remains pending.
   Same export online gate passes 543 initial and 180 resumed confirmed frames, resumed
   ticks 746/746, stable slots/characters and offline recovery. Both peers drop 240 messages
   from 1228/1226 sends, with maximum burst 24 and 60 ms send delay. Exactly one queued
   close, zero closed-send errors/browser exceptions, exit 0. Command: LOCAL_EXPORT=1
   CONFIRMED=1 COMBAT=1 RECONNECT=1 QUEUED_CLOSE=1 DELAY_MS=60 BURST_LENGTH=24
   NO_CLOSED_SEND_ERRORS=1 node /private/tmp/1_game3_online.cjs.
   Log: /private/tmp/game3-touch-shared-online.log; screenshots: /private/tmp/game3-online-bukEOi.
   Published through 2647139 with the existing game3 profile. nginx validation/reload passes.
   Remote WASM matches tested SHA-256
   ffcb5e4b1fea735600780a9602b4bc302c996584ccc48e6f6d0cbf9ce06524b5.
   Game3 pack remains fde60bf794d12796ff28fd1661e7afb3057337e41939f64f09c8ba157347c7df;
   protected original /game/ pack remains
   5d04f53109eaf4de6b76d55c951791a0bd1a60777068f0b3bd86d7bca6bcd795.
   Production touch run passes hidden-input neutrality, gesture cancellation, guard release,
   both shared-guard lift orders and 74-tick replay from 134 to 208 with matching checksums
   and EOF behavior; exit 0, no browser exceptions. Command: PRODUCTION=1 node
   /private/tmp/4_game3_touch.cjs. Logs: /private/tmp/game3-touch-publish.log and
   /private/tmp/game3-touch-production.log; screenshots: /private/tmp/game3-touch-hRGRXh.
   Production online run matches 549 initial and 180 resumed confirmed frames, resumed
   ticks 754/751. Each peer drops 240 messages from 1234/1232 sends, maximum burst 24,
   with 60 ms send delay. Queued-close recovery preserves slots/characters; peer closure
   reaches offline; zero closed-send errors/browser exceptions, exit 0. Command:
   CONFIRMED=1 COMBAT=1 RECONNECT=1 QUEUED_CLOSE=1 DELAY_MS=60 BURST_LENGTH=24
   NO_CLOSED_SEND_ERRORS=1 node /private/tmp/1_game3_online.cjs.
   Log: /private/tmp/game3-touch-online-production.log; screenshots: /private/tmp/game3-online-gpl8iJ.
   Native keyboard menu navigation
   remains egui's fixed keys. PM moveset parity, three-player and cross-network checks
   remain open; no fighter-mechanic equivalence is claimed by this input checkpoint.
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
   Falcon Dive input checkpoint: the new grounded sequence initially produced SpecialU ->
   Landing -> Stand without ascent or damage. run_special cleared ground support during
   zero-velocity startup; it now clears support at launch. Existing airborne behavior is
   unchanged. Added Dive catch / Dive whiff to Terrain & replay using existing trace controls.
   Each fixture runs 180 wire-quantized ticks from settled tick 60. Catch traverses SpecialU,
   GrabHold, Air, Landing, Stand and deals 18 damage; whiff lands before its airborne timeout
   and deals zero. Native tests assert both paths/ascent/damage/replay/EOF. Browser verifies
   all 180 checksums, steps each path, checks EOF and restores the identical start checksum.
   432 game + 74 shell tests and export pass. Logs: /private/tmp/game3-dive-final-tests.log,
   game3-dive-final-build.log and game3-dive-final-browser.log. Browser screenshots:
   /private/tmp/game3-dive-meGyon; runner: node /private/tmp/5_game3_dive.cjs.
   An earlier online run exposed repeated loads of missing optional assets/fx/fire.png;
   resource existence now gates loading and the existing procedural effect remains visible.
   Final browser run rejects these errors and passes with no browser exceptions. Gameplay
   tuning and PM equivalence are unchanged/unverified; docs/3_pm_baseline.md records the scope.
   Final export online Dive run observes SpecialU and GrabHold on both peers, matches 547
   initial and 182 resumed confirmed frames, and resumes at ticks 758/755. Each peer drops
   240 of 1245 messages with maximum burst 24 and 60 ms send delay. Queued-close recovery
   retains slots/characters; peer closure reaches offline. No missing-fire-texture errors,
   closed-send errors or browser exceptions; exit 0. Command: LOCAL_EXPORT=1 DIVE=1
   CONFIRMED=1 COMBAT=1 RECONNECT=1 QUEUED_CLOSE=1 DELAY_MS=60 BURST_LENGTH=24
   NO_CLOSED_SEND_ERRORS=1 node /private/tmp/1_game3_online.cjs.
   Log: /private/tmp/game3-dive-final-online.log; screenshots: /private/tmp/game3-online-5HRVUS.
   Published through 5e0eacd with the existing game3 profile; nginx validation/reload passes.
   Remote WASM matches tested SHA-256
   efb7b9963434b1c937b8ea4ee4762ed7f00b00d11dec7b05c310a866782f8066.
   Game3 and protected original /game/ pack hashes remain unchanged. Publish log:
   /private/tmp/game3-dive-publish.log. Production catch and whiff fixtures each verify all
   180 tick checksums and traverse the same state paths as the native/local fixtures.
   Restore and EOF pass; explosion screenshot shows victim damage 18. Both final checksums
   match the local export. Zero optional-fire-art errors or browser exceptions; exit 0.
   Command: PRODUCTION=1 node /private/tmp/5_game3_dive.cjs.
   Log: /private/tmp/game3-dive-production.log; screenshots: /private/tmp/game3-dive-x85uUc.
   Production online Dive run observes SpecialU and GrabHold on both peers, matches 547
   initial and 180 resumed confirmed frames, and resumes at ticks 760/758. Each peer drops
   240 messages from 1246/1243 sends, maximum burst 24 with 60 ms send delay. Queued-close
   recovery retains slots/characters; peer closure reaches offline. No optional-fire-art,
   closed-send errors or browser exceptions; exit 0. Command: DIVE=1 CONFIRMED=1 COMBAT=1
   RECONNECT=1 QUEUED_CLOSE=1 DELAY_MS=60 BURST_LENGTH=24 NO_CLOSED_SEND_ERRORS=1
   node /private/tmp/1_game3_online.cjs. Log: /private/tmp/game3-dive-online-production.log;
   screenshots: /private/tmp/game3-online-mTuCOZ.
   Next: cover startup on moving/drawn support and airborne entry before changing
   additional Falcon rules. PM timing/hitbox equivalence and physical-device checks remain open.
   Support-boundary follow-up adds stationary-ship, diagonally moving-ship and open-air
   up-special sequences. Each verifies startup support, launch and 90-tick checksum replay,
   including snapshot restoration after input index 12. The first launch expectation was one tick
   early: input enters frame zero, execution begins next tick. The corrected moving case
   exposed a 0.1584 px first-tick floor offset: grounded specials had not applied horizontal
   carry, but repin_ink_riders subtracted it as though they had. The post-hull correction now
   applies full translation for grounded specials. An intermediate special-branch carry
   attempt failed the teleport sweep's real-floor invariant and was removed.
   Final gate: 433 game + 74 shell tests pass, including the teleport sweep and all three
   support sequences. Runtime delta: +4/-1 in step.rs; no fields or tuning changes.
   Command: CARGO_BUILD_JOBS=1 just game-test. Log:
   /private/tmp/game3-dive-support-final-tests.log. Focused test:
   dive_input_preserves_ship_support_until_launch_and_replays_air_entry.
   Next: export/browser/online acceptance and publish this carry correction. Current
   production remains 5e0eacd; no browser run or deploy was performed for this change yet.
   Web follow-up: export succeeds with CARGO_BUILD_JOBS=1 just game3-build. Local browser
   catch/whiff fixtures each verify 180 ticks, preserve their prior final checksums, and pass
   restore/EOF without optional-fire-art errors or browser exceptions. Command:
   node /private/tmp/5_game3_dive.cjs. Logs: /private/tmp/game3-carry-build.log and
   /private/tmp/game3-carry-browser.log; screenshots: /private/tmp/game3-dive-StCiDe.
   This is runtime regression coverage; moving-hull startup itself is currently native-tested.
   Local export online regression observes Dive/catch on both peers, matches 551 initial
   and 180 resumed confirmed frames, and resumes at ticks 755/753. Each peer drops 240
   of 1236 messages with maximum burst 24 and 60 ms send delay. Queued-close recovery
   preserves slots/characters; peer closure reaches offline. No optional-art load errors,
   closed-send errors or browser exceptions; exit 0. Command: LOCAL_EXPORT=1 DIVE=1
   CONFIRMED=1 COMBAT=1 RECONNECT=1 QUEUED_CLOSE=1 DELAY_MS=60 BURST_LENGTH=24
   NO_CLOSED_SEND_ERRORS=1 node /private/tmp/1_game3_online.cjs. Log:
   /private/tmp/game3-carry-online.log; screenshots: /private/tmp/game3-online-TeDZH3.
   Published through b26def1 with the existing game3 profile; nginx validation/reload passes.
   Remote WASM matches tested SHA-256
   5defb6bcdb055f1232a9e66868f375d8e7264bc5e5723d19ae3a633f264b5ba4.
   Both Game3 and protected original /game/ pack hashes remain unchanged. Publish log:
   /private/tmp/game3-carry-publish.log. Production online regression observes Dive/catch on
   both peers, matches 549 initial and 180 resumed confirmed frames, and resumes at ticks
   759/757. Each peer drops 240 messages from 1239/1237 sends with maximum burst 24 and
   60 ms send delay. Queued-close recovery preserves slots/characters; peer closure reaches
   offline. No optional-art load errors, closed-send errors or browser exceptions; exit 0.
   Command: DIVE=1 CONFIRMED=1 COMBAT=1 RECONNECT=1 QUEUED_CLOSE=1 DELAY_MS=60
   BURST_LENGTH=24 NO_CLOSED_SEND_ERRORS=1 node /private/tmp/1_game3_online.cjs.
   Log: /private/tmp/game3-carry-production.log; screenshots: /private/tmp/game3-online-NCG1ve.
   Moving-hull debugger follow-up adds Ship Dive beside the catch/whiff fixtures. It starts
   Falcon on a translating hull and records 180 wire-quantized input ticks. The opt-in web
   debug snapshot exposes the hull position; simulation and network rules are unchanged.
   433 game + 75 shell tests and the production export pass. Local browser assertions at
   ticks 61..71 keep Falcon within 0.1 units of the expected hull-relative position while
   the hull moves; tick 72 lifts him more than 10 units off that floor. Catch, whiff and
   ship fixtures each verify/replay 180 ticks, restore the initial checksum and preserve
   the terminal checksum on EOF. No browser exceptions or optional-fire-texture errors.
   Ship trace: tick 60 checksum 53f46c886c9815b2b1759aeae045771f083e848b117a298ac31600edd4c9d43c;
   tick 240 checksum f4a0dc7c4f8c3647013f87300da1fd94a9ee16dbd24906c3549a182ea4d51fb4.
   Command: node /private/tmp/5_game3_dive.cjs. Logs:
   /private/tmp/game3-ship-fixture-final-tests.log, game3-ship-fixture-build.log and
   game3-ship-fixture-browser.log. Screenshots: /private/tmp/game3-dive-dBj5vD.
   Initial fixture placement accidentally allowed an opponent catch; corrected placement
   isolates hull startup. The failed fixture run supplies no passing acceptance evidence.
   Runtime/test delta before this ledger: +45/-6 lines across two shell files.
   Published through 83e3456 with the dedicated Game3 profile; nginx validation/reload passes.
   Remote WASM matches the tested export:
   3415d650919cf80ea9dbcd2f68f48b7dab8e76ec25a96fc34855539664a0dddb.
   Game3 pack remains fde60bf794d12796ff28fd1661e7afb3057337e41939f64f09c8ba157347c7df;
   original /game/ pack remains 5d04f53109eaf4de6b76d55c951791a0bd1a60777068f0b3bd86d7bca6bcd795.
   PRODUCTION=1 node /private/tmp/5_game3_dive.cjs passes all three 180-tick replays,
   moving-hull startup/launch assertions, restore and EOF, with the same endpoint checksums
   as local execution and no browser exceptions or optional-fire-texture errors; exit 0.
   Logs: /private/tmp/game3-ship-fixture-publish.log and game3-ship-fixture-production.log.
   Screenshots: /private/tmp/game3-dive-OtE1qQ. No new compiler job was required for publication.
   Cell/contact follow-up extends the existing playground regression without runtime changes:
   zero-based input indices 90 and 140 attack, 162 grabs with forward direction. All inputs
   pass through net::encode/decode. By index 159 Falcon holds the detached cell; the four
   original cell identities remain represented in terrain/items. At 162 the cell is thrown,
   retaining owner 0 for self-hit exclusion. At 163 it is consumed on impact: cell 18 takes
   the authored throw damage (currently 9), cells 19/20 remain undamaged, and Falcon remains
   at zero damage. Every subsequent checksum matches replays from the initial snapshot and
   snapshots after indices 139, 161 and 163, covering pickup, throw and post-contact restore.
   433 game + 75 shell tests pass, log /private/tmp/game3-cell-sequence-tests.log.
   Initial diagnostic runs retained the old final-state cell-presence assertion, which fails
   after the thrown cell is consumed; lifecycle assertions now check preservation before throw
   and consumption with target damage on contact. No simulation fix or PM equivalence claim.
   Test delta: +40/-23 lines; no export/browser/online rerun for this test-only change.
   Debugger follow-up: Cell replay loads the same playground_inputs iterator used by the native
   regression. Existing step/verify/restore controls execute all 240 ticks. Opt-in web snapshots
   expose terrain identities/damage and cell item ownership/thrown state; simulation rules and
   wire packets are unchanged. 433 game + 76 shell tests and production export pass.
   Local browser at tick 161 holds cell 17, tick 164 throws it with owner 0, and tick 165
   consumes it on contact with cell 18 (9 damage); cells 19/20 retain zero damage. The viewer
   shows cell 18 at 1 HP. Verify reports all 240 checksums matched. Replay reaches tick 241,
   EOF preserves its checksum, Restore returns the exact tick-1 snapshot. A live Step then
   Replay step refuses the changed state without mutation and displays the restore instruction.
   No browser exceptions; exit 0. Command: node /private/tmp/6_game3_cells.cjs.
   Logs: /private/tmp/game3-cell-trace-final-tests.log, game3-cell-trace-build.log and
   game3-cell-trace-browser.log. Screenshots: /private/tmp/game3-cells-EjgCQU.
   Start checksum: 897a702604273ba2aff48dfe9948c52d37ce04151d50fcc5586550ef6a6a92de;
   end checksum: 60c616e456951ffb58efd8b5c4a41ea7145294b00b6919cec90f7f175efa4935.
   Runtime/test delta before this ledger: +57/-10 lines across four files.
   Published through f9a486d using the dedicated Game3 profile; nginx validation/reload passes.
   Remote WASM matches tested SHA-256
   84a5d71d329289880f642ea9b8abac7d4a5bc400669213507520b5a157e7c065.
   Game3 pack remains fde60bf794d12796ff28fd1661e7afb3057337e41939f64f09c8ba157347c7df;
   protected original /game/ pack remains
   5d04f53109eaf4de6b76d55c951791a0bd1a60777068f0b3bd86d7bca6bcd795.
   PRODUCTION=1 node /private/tmp/6_game3_cells.cjs passes exact cell ownership, throw and
   impact readbacks, all 240 replay steps, unchanged EOF, restore and changed-state rejection.
   Start/end checksums match local execution above. No browser exceptions; exit 0.
   Logs: /private/tmp/game3-cell-trace-publish.log and game3-cell-trace-production.log.
   Screenshots: /private/tmp/game3-cells-WVSKHF. No rebuild or online rerun for publication.
   GGRS follow-up: cell_lifecycle_matches_offline_through_ggrs_rollback runs all 240 shared
   wire-input ticks through SyncTest/Game with check distance 7. Every saved-frame checksum
   (including re-simulation saves) and every final per-input state matches the offline trace.
   The test inspects actual LoadGameState requests and requires restores crossing input
   indices 90, 140 and 162, covering break, pickup and throw/contact. Test-only delta: +33 lines.
   434 game + 76 shell tests pass; /private/tmp/game3-cell-ggrs-final-tests.log.
   Initial distance 8 was rejected by GGRS because check distance must be less than the
   default prediction window; corrected to 7 without changing production configuration.
   No runtime/export/browser/deployment changes. This is all-local rollback with known inputs;
   this fixture's delayed-input correction and two-peer online cell-contact remain unverified.
   Missing-input follow-up adds three handler-level correction cases at input indices 90,
   140 and 162. Each saves the boundary state, advances seven neutral Predicted input ticks,
   saves each resulting frame, then loads the boundary and delivers the recorded Confirmed
   inputs through the remaining 240-tick sequence. Predicted endpoint checksums must differ
   from corrected endpoints. Every corrected state, overwritten saved cell and observer-map
   receipt matches independent offline stepping. 435 game + 76 shell tests pass;
   /private/tmp/game3-cell-correction-tests.log. Test-only delta: +44 lines.
   No runtime change, export, browser run or publication. Requests are explicitly constructed
   through the existing GGRS Game handler; transport scheduling and two-peer cell contact are
   not covered by this receipt.
   Two-peer production follow-up: examples/0_cell_start.rs writes the existing playground
   state as a 46,740-byte bincode snapshot. From the repository root:
   CARGO_BUILD_JOBS=1 cargo run -p kneeman --example 0_cell_start > /private/tmp/game3-cell-start.bin.
   The ad-hoc runner encodes that snapshot into the initial private-room offer; the existing
   offer/answer path gives both peers the same state. No runtime/startup protocol changes.
   CELLS=1 CONFIRMED=1 COMBAT=1 RECONNECT=1 QUEUED_CLOSE=1 DELAY_MS=60 BURST_LENGTH=24
   NO_CLOSED_SEND_ERRORS=1 node /private/tmp/1_game3_online.cjs passes on published f9a486d.
   Both peers reach cells 18/19/20 with damage 9/0/0, empty cell-item pools and no held item.
   542 initial confirmed frames match; each peer drops 240 application messages from 1225/1229
   send attempts, maximum burst 24, with 60 ms send delay. After the queued channel close,
   180 resumed frames match at game ticks 750/750; handles/characters remain stable and peer
   closure reaches offline. No closed-send errors or browser exceptions; exit 0.
   Log: /private/tmp/game3-cell-online-fixture.log; screenshots: /private/tmp/game3-online-NkT7Uy.
   Initial browser preflight expected an offline raw snapshot property that is not exposed;
   it stopped before matchmaking and supplies no online evidence. The native exporter uses
   the public terrain_cells API from the root workspace; export log:
   /private/tmp/game3-cell-start-build.log. 435 game + 76 shell tests pass after adding it,
   /private/tmp/game3-cell-online-native-tests.log. No web rebuild or publication was needed.
   The browser asserts the final cell-contact result and confirmed-state agreement; individual
   one-tick pickup/throw visuals are covered by the separate paused replay receipt above.
   Special-slot inventory is now in docs/3_pm_baseline.md: neutral/side/down still inherit
   PUNCH/LUNGE/DROP; only up-special is replaced. Next move selected: Falcon Kick, including
   airborne jump restoration and ground/air behavior. The PM character page confirms jump
   restoration but provides no exact reset frame. Melee's SpecialLw callbacks show separate
   phases; animation-command/generic callback tracing is needed before assigning timing.
   Next: trace the jump-count write and form an air-jump -> down-special -> air-jump sequence,
   with interruption/landing cases, without changing KneeMan's inherited Fall behavior.
   This inventory changed no runtime or deployment and required no compiler/browser run.
   Physical/cross-network devices and exact Project M move data remain unverified.
   Kick restoration follow-up traced the Melee callback: SpecialAirLw_Anim calls
   ftCommon_8007D5D4 at entry to the ending animation; that helper sets jumps used to one.
   Source links and the Game3 timing difference are recorded in docs/3_pm_baseline.md.
   Falcon now selects an appended FallRefreshJump kind for its down-special; airborne
   completion restores configured air jumps. Motion/hit data remain the inherited DROP data.
   A 60-tick semantic-input test spends jump -> down-special -> jumps again, both facings,
   with KneeMan as a non-restoring control and full-state replay after mid-move restoration.
   The test failed before implementation; native receipts: /private/tmp/game3-kick-before.log
   and /private/tmp/game3-kick-tests.log. Runtime/test delta: +55/-5 across three files.
   Publication pending. Next: ground/air/interruption and loadout-isolation cases, then browser
   and online checks including mixed-build startup rejection for the appended enum variant.
   This partial restoration does not complete Falcon Kick's separate travel/end/landing phases.
   Boundary/loadout follow-up: the existing 60-tick test also swaps only the down-special
   kind, enabling restoration for KneeMan and disabling it for Falcon. Both facings follow
   the selected data and retain per-tick checksum replay. Direct move-boundary checks with
   0/1 remaining jumps verify no restoration one frame early, no additional restoration while
   grounded, and restoration on airborne completion. A serialized Launched/hitstun snapshot
   retains zero jumps across 20 airborne replay ticks; this models the interrupted state,
   not an actual attack connecting during the kick. 437 game + 76 shell tests pass;
   /private/tmp/game3-kick-boundary-tests.log. Test-only change; no runtime/deployment change.
   Browser and mixed-build startup gates remain pending, as do actual hit interruption,
   landing during kick and the separate ground/air move phases.
   Actual-hit follow-up adds a 40-tick two-fighter input sequence: Falcon down-special and
   the opponent's aerial start together. Combat changes SpecialD to Launched, increases
   damage and sets hitstun. Falcon stays airborne with zero jumps throughout; every state
   matches replay, including restoration from the impact snapshot. The initial opponent
   attack at index 3 lost to the kick and only caused attacker hitlag; it did not meet the
   interruption assertion. Moving the opponent input to index 0 exercises the intended hit.
   438 game + 76 shell tests pass; /private/tmp/game3-kick-hit-final-tests.log.
   Test-only delta: +36 lines. No runtime or publication change.
   Next: rebuild the export and verify new/old pair startup rejects the unknown move kind
   without desync or crash, then browser-test the kick recovery on matching builds.
   Export/mixed-build follow-up: game3-build passes for a1dc227, log
   /private/tmp/game3-kick-build.log. Local WASM SHA-256:
   05afb1b89708a9ef177f3b25ce34a9a84b8974ea7bc1ca085543c13efbeb87dc.
   One local-export host paired with one published guest rejects startup exactly once with
   Invalid startup Tune, then both clients run offline for more than 30 further ticks without
   browser exceptions. No corrupt-packet mutation is injected: BAD_START=mixed selects the
   existing rejection assertions while MIXED routes only one context to the new export.
   Command: LOCAL_EXPORT=1 MIXED=1 BAD_START=mixed RECONNECT=1
   node /private/tmp/1_game3_online.cjs. Log: /private/tmp/game3-kick-mixed.log;
   artifacts: /private/tmp/game3-online-b1bgD2. Exit 0.
   Reverse direction (published host, new guest) passes 543 initial and 180 resumed confirmed
   frames, game ticks 750/749. Each peer drops 240 messages from 1225/1226 sends, maximum burst
   24 with 60 ms delay. Slots/characters survive queued channel close/reconnect; peer closure
   reaches offline. No closed-send errors/browser exceptions; exit 0. This verifies compatibility
   with the old host's startup data, not availability of the new kick behavior in that session.
   Command: LOCAL_EXPORT=1 MIXED=old-host CONFIRMED=1 COMBAT=1 RECONNECT=1 QUEUED_CLOSE=1
   DELAY_MS=60 BURST_LENGTH=24 NO_CLOSED_SEND_ERRORS=1 node /private/tmp/1_game3_online.cjs.
   Log: /private/tmp/game3-kick-old-host.log; artifacts: /private/tmp/game3-online-6ujM5n.
   No publication or game-source changes. Next: matching-new-build kick recovery acceptance;
   retain the partial-move limits in docs/3_pm_baseline.md.
   Matching-build kick recovery: examples/1_kick_start.rs serializes the native test's
   airborne Falcon start through the public game API. Build from repository root with
   CARGO_BUILD_JOBS=1 cargo run -p kneeman --example 1_kick_start > /private/tmp/game3-kick-start.bin.
   Build/run passed; /private/tmp/game3-kick-start-build.log. The existing private-room
   startup injection sends this snapshot, and semantic inputs jump at 0, down-special at 3,
   then jump at 45. Both new-build peers show the first ascent, SpecialD, no sampled Stand/
   Landing through tick 55, then a second ascent exceeding 30 units from the pre-jump low point.
   This is sampled browser motion evidence combined with the native spent-jump assertions.
   Initial runner compared ascent against an earlier higher position and failed despite
   observing velocity reversal; corrected comparison uses ticks 45..48 versus 51..55 and
   explicitly verifies the first jump. No simulation change was made for that correction.
   LOCAL_EXPORT=1 KICK=1 CONFIRMED=1 COMBAT=1 RECONNECT=1 QUEUED_CLOSE=1 DELAY_MS=60
   BURST_LENGTH=24 NO_CLOSED_SEND_ERRORS=1 node /private/tmp/1_game3_online.cjs passes:
   546 initial and 180 resumed confirmed frames match, game ticks 753/752. Each peer drops
   240/1243 messages, maximum burst 24. Fighter slots/characters survive reconnect, peer close
   reaches offline, no closed-send errors or browser exceptions; exit 0.
   Log: /private/tmp/game3-kick-browser-rise.log; artifacts: /private/tmp/game3-online-21yx68.
   Only the 14-line native fixture exporter was added; game runtime and tested export remain
   unchanged. No new full native gate this turn; latest gate remains 438 game + 76 shell.
   Next: publish the tested kick export and repeat matching-build recovery assertions on
   production. This acceptance does not establish complete Falcon Kick phases or PM timing.
   Online was not rerun for this debugger-only change; the preceding published-runtime
   receipt remains the latest online evidence. Physical devices and PM parity remain open.
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
   Host-tab replacement fails on the published build after 549 initial matching frames.
   Both clients report Guest (handle 1), no offer/resume follows, and both eventually go offline;
   the resumed-running assertion times out at 45 seconds, runner exit 1.
   Command: CONFIRMED=1 COMBAT=1 RECONNECT=1 REPLACE_TAB=1 REPLACE_HOST=1 node /private/tmp/1_game3_online.cjs.
   Log: /private/tmp/game3-host-replacement.log; screenshots: /private/tmp/game3-online-GdOtuP.
   Client cause: rtc::resolve_rematched(true, 2, 1, (0, Host)) returns (1, Guest).
   mesh::handle_signal applies that override to the surviving guest, while the fresh page
   takes the relay's Guest assignment. Existing rtc tests explicitly pin the old override.
   Next fix: separate relay offer/answer role from persistent fighter slot; exchange/adopt the
   retained snapshot and slot assignment before starting GGRS. Cover both join orders, either
   tab replacement, retained-tab reconnect, character-slot continuity and Tune authority.
   Changing only the role override would permit fighter swaps and can select a fresh host's
   spawn/default Tune over the surviving match. No runtime/server change made for this receipt.
   Remaining input customization and burst-loss acceptance follow this reconnect fix.
   Local fix: transport roles now follow the relay while fighter slots remain independent.
   Pair SDP carries start_version=1, slot, optional snapshot and Tune. The answerer selects
   the offer's retained state when present, otherwise its own retained state; fresh pairs use
   the offer's Tune. Both roles wait for this startup exchange before creating GGRS state.
   Pair-only legacy resume/Tune frames are ignored; mixed old/new clients require reload.
   Unknown startup versions and malformed startup payloads reject back to offline.
   432 game + 72 shell tests pass, including retained-slot policy for both join orders,
   either replaced slot and invalid/conflicting claims. Log: /private/tmp/game3-pair-start-tests.log.
   Rebuilt export + existing relay now passes the reproduced host replacement: 549 initial
   and 181 resumed confirmed frames match, resumed ticks 760/759, original fighter handles
   and character slots retained, subsequent peer close reaches offline; runner exit 0.
   Command: LOCAL_EXPORT=1 CONFIRMED=1 COMBAT=1 RECONNECT=1 REPLACE_TAB=1 REPLACE_HOST=1 node /private/tmp/1_game3_online.cjs.
   Log: /private/tmp/game3-host-replacement-fixed.log; screenshots: /private/tmp/game3-online-wk89Ru.
   The attempted forced-order/nondefault-Tune test using Playwright routeWebSocket never reached
   initial matching and timed out at 90 seconds. It provides no join-order/Tune evidence.
   Log: /private/tmp/game3-fresh-host-tune.log; screenshots: /private/tmp/game3-online-kIWMus.
   Retained-tab reconnect passes on this export too: 549 initial and 180 resumed confirmed
   frames, preserved handles/characters, game ticks 745/746, then offline timeout recovery;
   runner exit 0. Log: /private/tmp/game3-pair-start-retained.log;
   screenshots: /private/tmp/game3-online-EnKfpf. Same command without REPLACE_TAB/REPLACE_HOST.
   Initial publication gates (receipts below): verify the fresh-transport-host ordering with a working
   harness, nondefault Tune retention, guest replacement, malformed/start-send rejection and
   retained-tab reconnect under the final artifact. The old FAIL_RESUME_TUNE injector targets the superseded pair message;
   adapt it to SDP/start send failures. Server configuration remains unchanged.
   Reverse-order follow-up: a browser-local delayed WebSocket constructor makes the fresh
   replacement arrive before the survivor. Host replacement now passes 549 initial and 181
   resumed confirmed frames, game ticks 760/760, unchanged fighter handles/characters and
   timeout recovery. Initial offer gravity is changed to 17.25 (valid Tune first f32);
   the fresh host later offers default 4284, while the survivor's retained answer supplies
   17.25 and a snapshot. Both sessions agree. Runner exit 0, no browser exceptions.
   Command: LOCAL_EXPORT=1 CONFIRMED=1 COMBAT=1 RECONNECT=1 REPLACE_TAB=1 REPLACE_HOST=1 FORCE_FRESH_HOST=1 TUNE_RECEIPT=1 node /private/tmp/1_game3_online.cjs.
   Log: /private/tmp/game3-fresh-host-tune-fixed.log; screenshots: /private/tmp/game3-online-cxwfB1.
   The preceding native-wrapper attempt lacked instance OPEN/CLOSING constants required by
   GodotWebSocket.send/close and failed before sending the answer; /private/tmp/game3-fresh-host-native-socket.log
   is a harness failure. Correcting those constants required no game-source change.
   Malformed startup browser gates pass sequentially for version=0, slot=8, invalid state
   and invalid Tune. Each produces exactly one named startup rejection; both peers return
   offline and advance more than 30 ticks afterward, no browser exceptions, runner exit 0.
   Commands: LOCAL_EXPORT=1 RECONNECT=1 BAD_START={version,slot,state,tune} node /private/tmp/1_game3_online.cjs
   (run one value at a time). Logs: /private/tmp/game3-bad-start-{version,slot,state,tune}.log.
   Screenshots respectively: /private/tmp/game3-online-EK3gex, game3-online-XcbVQP,
   game3-online-hgtoHE and game3-online-n70Cmm, all under /private/tmp.
   SDP/start send failure also passes: after 549 confirmed combat frames, report 256 KiB
   bufferedAmount on one reconnect signaling socket. Exactly one ERR_OUT_OF_MEMORY is logged;
   both peers return offline and advance more than 30 ticks from 944/942, no browser exceptions.
   Runner exit 0. Command: LOCAL_EXPORT=1 CONFIRMED=1 COMBAT=1 RECONNECT=1 FAIL_START=1 node /private/tmp/1_game3_online.cjs.
   Log: /private/tmp/game3-start-send-failure.log; screenshots: /private/tmp/game3-online-JN2fqL.
   Guest replacement with fresh-first ordering passes too: 549 initial and 181 resumed
   confirmed frames, ticks 759/759, preserved handles/characters and retained gravity 17.25
   despite the new transport host offering 4284. Subsequent close recovers offline, exit 0.
   Same reverse-order command without REPLACE_HOST. Log: /private/tmp/game3-guest-replacement-reversed.log;
   screenshots: /private/tmp/game3-online-2XXzwM. Tested export WASM SHA-256:
   7526f6b3c2d4a009405cb6063a80405a7c06e70e63007a80d299e0466b4425b0.
   Published the tested export on 2026-09-06 using the existing game3 profile; nginx validation
   passed. Remote WASM matches that hash; Game3 and protected original /game/ packs are unchanged.
   Cross-network/physical-phone and burst-loss checks remain open.
   Production-only reverse host replacement passes after publication: 549 initial and 180
   resumed confirmed frames, ticks 761/759, stable handles/characters, retained gravity 17.25
   over the fresh host's 4284 proposal, then offline timeout recovery. Exit 0, no browser exceptions.
   Same command without LOCAL_EXPORT. Log: /private/tmp/game3-pair-start-production.log;
   screenshots: /private/tmp/game3-online-43Wya7. Reload both clients for start_version=1.
   Next bounded implementation: gamepad binding customization in controls/0_bindings.rs,
   controls/mod.rs and ui/menu/controls.rs, keeping the semantic InputFrame unchanged.
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
