# Falcon locomotion delivery

Predecessor: `109_tasks.md`; `104_tasks.md` and `102_tasks.md` retain carried
G/P requirements. Domain execution and receipts: `../../4_tasks.md` (F1-F5).
The later user authorization supersedes 109's stop-after-L2 condition.

| ID | State | Terminal condition / checkpoint |
| --- | --- | --- |
| F1 | Done, bounded | PM3.6 numeric attributes retained and generated; common callback equivalence remains explicitly unqualified. `53705b2`. |
| F2 | Done, bounded | Eighteen action pages decode with exact frame counts and checked hashes. `efa8559`, `53705b2`. |
| F3 | Done, bounded | `3dc9886`: shared fighter Redux state consumed by Falcon; `just test core` passed, `.workflow/test-UrhSp3/receipt.json`. Source-equivalence limits remain in domain ledger. |
| F4 | Done, bounded | Browser dash/run poses, facing, speed, observed graph and inspected MP4 passed in `.workflow/prove-lsenEc/receipt.json`. Grounded statig followup `6c7709f` also passed six stages in `.workflow/prove-MifHpW/receipt.json`. Preserve historical knee proof. |
| F5 | Done, prior visualizer | `763f336` published with `.workflow/deploy-mEG4UH/receipt.json`; production acceptance and protected `/game/` hashes passed. New ground/static/runtime increment needs its own source-bound proof and deployment. |

F1/F2 qualify the selected locomotion data only. Full Falcon callback execution
and G2 inventory remain pending. G0/I1-I3/L1/N1/L2 retain completed evidence.
G1 stays deferred; G3-G6 pending, G7 on deck; P1/P2/P4 pending, P3 partial.
Existing v1/v3 extraction, item/terrain and cross-target network goals remain
queued. No GUI launch, new networking, attacks beyond existing fair, or site-wide
acquisition is authorized by this slice. Ingest runs offline; SQLite publishes
derived presentation state after the Rust simulation advances.

Current visualizer increment: static graph derived from executable grounded
decisions; runtime Godot observer shows phase-clock re-entry (including DASH to
DASH), entry tick and observed edge counts. Headless Godot/controls gate passed
in `.workflow/test-clv3oo/receipt.json`. Static export and full visual proof pending.
These are observed presentation samples, not a lossless transition event stream.

S5 subsequent delivery: `a5ce0af` static/runtime visualizers were proved and
deployed; domain `../../4_tasks.md` records the source-bound receipts and MP4.
This supersedes the pending proof/deployment text above.

Opus `b099bf2` + `73c1cf6` add `godot/4_dash_dance.test.gd`, using real
FalconSql controlled ticks and generated Rows/Payload adapters. Independent
expectations: DASH re-entry T6, RUN T21, TURN T26, direct TURN->SQUAT T27,
takeoff T30, five observed edges, and rewind clearing the observer graph.
Coordinator rebuilt the primary native extension with `just godot-build` and
passed `just test-godot`, including the required `DASH_DANCE_OK` marker.
Package: Falcon lab (app proof), stage 2.7 -> 2.7. Property gained: actual
simulation-to-observer regression, including same-phase re-entry and Turn jump.
No runtime behavior changes or new deployment are claimed by this test increment.
Full `just test` passed core, workflow, Godot and controls:
`.workflow/test-OL4Zdg/receipt.json`. This receipt predates only this ledger note.

F41 browser dash dance fix: the controller input adapter summed opposing digital
directions, so holding right and pressing left emitted axis 0 and the Dash
reverse self-transition never reached the reducer. `godot/2_input.gd` resolves
two opposing digital sides by last-pressed order with a deterministic focus
reset; `2_stage.gd` feeds it key events and touch buttons. Native
`3_control.test.gd` covers overlap and blur reset; `94_web/4_browser.mjs` drives
a real headless browser and asserts four reversals inside Dash before Run, with
phase-clock restarts, facing flips and both DOM keys held. No reducer, chart or
deployment change.
