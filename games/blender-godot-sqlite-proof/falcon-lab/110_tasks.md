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

`godot/4_dash_dance.test.gd` closes the observer's real-boundary gap: actual
`start_controlled(false)`, `advance_controlled` and `acknowledge` over a fixed
dash-dance tape, published `phase`/`phase_ticks` fed to the stage observer.
Receipts: DASH re-entry at T6 with the phase clock restarting at 1, RUN at T21,
TURN at T26, SQUAT/JUMP at T31/T34, six observed edges, and a re-presented
earlier sample clearing the graph. Wired into `just test-godot`, gated on the
`DASH_DANCE_OK` line because Godot exits 0 on a failed script assert. The lane
reused the primary checkout's `libfalcon_gdext.dylib` and contracts
`node_modules` at the same commit; a source-bound native rebuild and full
`just test` rerun stay with the parent.
