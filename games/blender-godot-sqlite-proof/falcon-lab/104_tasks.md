# Reusable foundations through playable slices

Recorded 2026-09-08. Predecessor: `102_tasks.md`, retained with its evidence and
acceptance details. This ledger supersedes its execution order: buffered input
comes first; full-site acquisition is deferred and does not gate playable work.
The working rules and unsuperseded acceptance criteria in `102_tasks.md` remain.

## Immediate slice: buffered input to Falcon action

2026-09-09 statechart qualification requested separately from I2: macro-free
statig 0.4.1 integrated as a dev-only test in
`../../shared/redux/tests/2_statechart.rs`. Existing Slice and Then APIs unchanged;
one Slice dispatches to the chart and emits inert effects; a following slice
returns the step count. World owns machine state; rules are borrowed per step.
No rxRust work is requested. Falcon runtime remains unchanged.

Executed: `cargo test --manifest-path games/shared/redux/Cargo.toml --features
serde --offline -j 2` from hafley-rs: 16 unit + 1 existing integration + 3 new
statechart tests passed. Scoped Clippy `--test 2_statechart -- -D warnings`
passed. Tests cover zero/two-tick policy, expiry, single consumption, parent-state
cancellation, Then output, and clone-based restore with matching state/effects.
Serialization caveat is a passing diagnostic regression: statig deserialization
marks the machine uninitialized and reruns entry actions on next dispatch;
entry counts diverge (clone 2, deserialized 3). Raw Serde restore is therefore
not qualified for rollback. No GGRS transport, allocation benchmark, TSP
generation or MP4 integration was performed by this nonvisual API lab.

Follow-up GGRS qualification (2026-09-09): the same test fixture now implements
shared `RollbackSim` and runs through `synctest_session` and `Game::handle_observed`
to existing `apply_request`. Four runs (window 0/2, check distance 1/7), each 64
forward ticks, compare complete chart state, entry count, step count and effect
history against uninterrupted execution after every frame. Actual Save/Load/
Advance requests and replay advances are asserted; distance 7 additionally
compares repeated saved-frame checksums. The four statechart tests passed.
Distance 1 does not repeat saves (observed 64 saves, 62 loads, 126 advances), so
the replay assertion uses load/advance counts rather than assuming duplicate saves.
Snapshots use Clone, never Serde decode. This qualifies SyncTest restore/replay;
delayed remote-input correction, network transport, Falcon integration and MP4
remain untested. GGRS/rollback are test-only Redux dependencies.

Committed qualification and task reorientation: `639083d`. Continued I2 boundary
inspection found Falcon `simulation-core/2_simulation.rs::World` derives Serde
alongside Clone. Added `ggrs_restore_detects_accidental_serde_reinitialization`:
after a real GGRS Load request, deliberately round-trip the restored chart through
Serde and detect excess entry actions against uninterrupted state. All five
statechart tests and scoped Clippy passed. Preserve both positive clone and
negative Serde regressions. Falcon integration must explicitly preserve restore
semantics before adding entry actions; production runtime is still unchanged.

| ID | State | Work | Completion / checkpoint |
| --- | --- | --- | --- |
| I1 | Done | Trace existing buffered input and source tests; inspect v3 counterpart | `105_input_slice.md`: three source checks complete. User subsequently requested statig/Redux and GGRS qualification, then continuation. Source tests read, not newly executed. |
| I2 | Done | Extract required input machinery into `games/shared` and wire Falcon | Shared input buffer consumed by existing Redux Step; generated policy/inspection types; full clone/JSON/bincode restore and GGRS/SQL tests pass. See increment below. |
| I3 | In progress | Prove one buffered-action scenario | Deterministic proof passes; `just buffer-proof` added. GPU recording and frame inspection next. |

Scope: one buffered behavior, selected from inspected source evidence. Its exact
Falcon mapping is proposed in I1. Do not assume the existing input crate already
contains buffering. No wall-clock timers in authoritative window state. Report
current state during the work so the coordinator can interrupt or redirect drift.

## Carried goals and gates

| ID | State | Disposition |
| --- | --- | --- |
| G0 | Done | Shared Redux/GGRS integration; preserve `62832dc` and predecessor receipts. |
| G1 | Deferred | Existing Falcon subset verified; full-site completion, retrieval metadata and real downloader recovery remain unverified. Resume for a concrete missing/corrupt asset or explicit acquisition request. Deferral does not establish background downloader status. |
| G2 | Pending | Expand versioned Falcon package and supported behavior after input slice; 491 decoded subactions, seven runtime actions. Full mirror is not a prerequisite. |
| G3 | Pending | Reuse existing cascade for item/region rules and generated IO. |
| G4 | Pending | Snapshot-owned stage/chunk terrain; inspect existing physics/geometry before implementing. |
| G5 | Pending | Boots pickup/drop changes air-jump rules; reuse input slice. |
| G6 | Pending | Integrated boots/terrain MP4 and rollback/SQL/mesh gates. |
| G7 | On deck | Moving ship and larger-world qualification after G6. |
| P1 | Pending | Shared TSP generation glue, non-Falcon fixture and Falcon consumer. |
| P2 | Pending | Shared SQLite ring/query/publication, preserving existing core-labs tests. |
| P3 | Partial | GPU/FFmpeg recorder extracted to shared/capture and consumed by existing recorder and I3; independent mesh test passes. Workflow fingerprint extraction retained; remaining receipt tooling pending. |
| P4 | Pending | Shared web driver/local export guards; no deployment required. |

All predecessor deferrals, browser/photo goals, protected deployment paths and
source-game equivalence caveats remain in effect. No new runtime or MP4 proof
is claimed by this planning update.

Workflow issue fixed: `just status` treated a Git submodule entry as a file,
causing `EISDIR`. Shared `../../shared/workflow/0_fingerprint.mjs` now hashes the
gitlink pin and initialized checkout recursively, including dirty/untracked
inputs; uninitialized submodules have an explicit marker. The Falcon workflow
uses it directly. `node --test 100_workflow.test.mjs`: 3 tests passed, including
local-submodule pin/edit/untracked/deinit regression. `just status` passed.
This completes the fingerprint extraction only; the remaining P3 gate stays open.

## Legacy extraction register

Source paths below are relative to sibling `hafley-rs-game-runtime/`, read-only.
Preserve useful capabilities here as they are discovered; record destination,
consumer, tests and disposition when extracting. This register is incremental,
not a completed audit of v1/v3.

| Capability | Source evidence | Task / status |
| --- | --- | --- |
| Quantized participant input | `crates/input/src/{0_types,1_quantize}.rs`: four i8 axes, u32 buttons and conversion | Ported to shared/input; richer four-axis Falcon mapping remains pending |
| Pressed/held and directional input | `games/kneeman/src/0_input_frame.rs` | I1; richer application input remains source-owned |
| Buffered action / coyote timing | `games/kneeman/src/v1/{physics,replay_tests,di_tests}.rs` | I1 first traces one behavior; remaining useful behavior stays pending |
| v3 input and game utilities | `smashy/reference/og-v1/gdscript-labs/v3-game-dom-css-lab/`, details in `105_input_slice.md` | Input edges/routes inspected; authored buffer FSM documented as pending. Other utilities remain pending inspection. |
| Typed rule resolution | `games/cascade`: gravity property, ordered selectors, source inspection | G3; additional properties/operators unverified |
| Contact, sweep and solver utilities | `games/physics2d/src/`: integration, basis, sweep, snapshot island | G4; review suitability and source tests before extraction |
| Redux and rollback | `games/shared/0_reuse.md` in active repository | G0 done; preserve integrated tests |
| Generation, storage, recording, web tooling | Predecessor P1-P4 source/acceptance descriptions | P1-P4 pending |

## Repository dependency checkpoint

Rukaidata source submodule: `../vendor/rukaidata`, remote
`https://github.com/rukai/rukaidata.git`, revision
`b0d6dd0999a28760ba21983a298cad44ad9ecff7`. Added through Git; source code and
downloaded fighter payloads remain separate dependencies. Submodule declaration
was staged before this planning update; no commit is claimed here.

## I2 integration receipt (2026-09-09)

Production shared `Buffer::advance(press, eligible, cancel, window) -> Outcome`
uses macro-free statig with no entry/exit actions. Falcon World owns the optional
buffer and policy; existing Redux Step advances it. Game owns eligibility and
aging. V1 replacement/age-before-record distilled with provenance in
`../../shared/0_reuse.md`. The buffer counter includes the press frame plus N
following ticks; no Melee/PM/Ultimate policy equivalence is claimed.

`106_buffer.rs`: same 180-tick tape, lab windows 0/8, actual GGRS SyncTest distance
7. Every corrected World equals direct execution; published SQLite rows and
derived mesh equal direct rows/mesh. Five checkpoints per policy replay through
clone, JSON and bincode restoration. Default unbuffered World also binary-round-
trips. Optional fields are serialized explicitly to preserve binary field layout.
Historical binary snapshots predating this schema change are not supported.
Existing raw-Serde/entry-action negative qualification test remains intact.

`just test core` passed: `.workflow/test-AXxLQg/receipt.json`, 28 Falcon tests,
3 shared input tests, 1 shared capture test, existing Redux/rollback and generator
tests. `just test-godot` passed outside sandbox after its user-log access failed
inside sandbox. Initial core receipt rejected source edits during the run; the
fresh stable-source run above passed. No network, web deployment or interactive
policy picker was added. Buffer proof is the new runtime consumer; ordinary
unconfigured worlds retain immediate-input behavior. I3 recording pending.
