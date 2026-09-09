# Smash promotion and qualification

Predecessors: [Falcon 109](blender-godot-sqlite-proof/falcon-lab/109_tasks.md),
[Falcon 104](blender-godot-sqlite-proof/falcon-lab/104_tasks.md).
Read N=2 preceding ledgers. Older G1–G7/P1–P4 unfinished work remains recorded there;
map each to this game's milestones during A1, without silently marking it done.
L1/N1/L2 remain completed historical evidence. This ledger replaces the next-lab
direction with the approved Smash app destination.

| ID | State | Terminal condition / checkpoint |
| --- | --- | --- |
| R1 | Done | D2 roadmap, evidence, destination rules and queued labs added. `just map` and `just test-map` pass; SVG inspected through `just map-png`. App creation/migration remain A1. |
| R2 | Done | Native TSP registry validates 9 existing packages + 1 app proposal through Cargo metadata; generates JSON/D2. Four test groups pass, including schema/identity/evidence/destination/staleness rejection. Populated SVG rendered and inspected. No scanner, package migration or automatic promotion. |
| R3 | Done | Independent Astra audit found annotation-dependent schema checks, referenced-constant duplicate bypass, unrelated-table task acceptance and unchecked SVG freshness. Fixed with authoritative assignability, inline records, rolling ID-table lookup and temporary SVG comparison. Six test groups and `just test-map` pass; package moves remain A1. |
| A1 | In progress | Shared crates moved in `b1c2046`. Falcon simulation moved into `smash/src/fighters/falcon`; lab/web imports use the app package, with no app dependency on the lab. Core tests passed (`falcon-lab/.workflow/test-FNxTcD/receipt.json`); pure WASM check passed. App registry 1 -> 2.7 carries the existing bounded slice evidence. Remaining: character-selectable offline ingestion, app executable consuming existing slice, remaining lab library/app splits. Report each move/dependency checkpoint before implementation. |
| Q1 | Queued, first library lab | Pin ssbm_utils APIs already used. Table-driven fixtures for knockback, Sakurai angle, launch velocity, hitstun and target weight. Label Melee results separately from PM comparisons. Report mismatches and missing authoritative PM fixtures; do not invent expected values. |
| Q2 | Queued | Baked pose/action fixtures verify bone-attached hurt/attack geometry, root motion, freeze/rate/transition and restore. Separate cosmetic interpolation. MP4 labels reflect actual action/frame/contact. |
| A2 | Queued | Walk/jump/crouch on Small Battlefield with facing-relative attack/special input selection. Down-tilt interpretation and PM smash/tilt windows recorded. Restore input history and transitions. Yield when routing is proven before expanding moves. |
| A3 | Queued | Side smash, down tilt, nair, fair and bair use imported timings/geometry with actual hit checks. Record directional input, action/frame and damage in MP4; replay same state. Enumerate special routing versus special execution coverage. |
| Q3 | Queued | Validate complete current slice replay, native/WASM equality and connected target compatibility separately. Measure normal and rollback bursts/allocations against frame budget on named hardware. Report each gate independently. |
| Q4 | Queued | Extract v1 items + Lovers ship behavior: exact source symbols, ownership/helm/control/modifier semantics, reusable destination and retained interaction fixture. No abandoned-app revival. |
| Q5 | Queued | Extract v3 destruction/input/support behavior: exact sources, shape-to-debris lifecycle and snapshot/input-feel fixtures. Moving/destroyed support survives replay. |
| Q6 | Queued, rolling bounded inventory | Triage inventory families below. One selected dependency per lab; pin revision/license/API and execute a relevant fixture before integration. Yield a result before selecting the next candidate. |
| M1 | On deck after A3 | Two-fighter match, stocks, blast zones and respawn. Full PM Falcon state machine remains a later coverage expansion. |

## Q6 families, not claims of qualification

A1 move verification: `just test core` and `just test-simulation-wasm` from the
Falcon lab passed. The separate `94_web/Cargo.toml` native `cargo check` passed
with the existing locked/offline two-job helper. `games/just test` passed all six
registry groups and map freshness. Registry dependency blocks in all three
affected lockfiles are unchanged. Browser execution, web adapter WASM export,
deployment and new MP4 capture were not run for this source-ownership move.
Promotion/session flow is committed in `cbf334c`, linked from `games/AGENTS.md`.

A1 content increment: `crates/content` owns the existing Action/Frame/Attack wire
types, path-selected decoder and baker. Ingestion is feature-gated; Smash depends
only on default content types. Lab decoding and baking consume the shared code.
The reference simulation and snapshot test now bake once before replay loops.
Three ingest tests cover seven retained actions, malformed/trailing payloads and
a synthetic identity; a real second-character fixture remains unqualified.
The standalone app executable, general import/export command and pose-content
promotion remain pending. Existing `falcon-import <directory>` uses shared decode.
Native web-adapter check and pure WASM check passed. The WASM normal dependency
tree excludes brawllib_rs, base64, bincode, wgpu and winit. Registry/map checks pass.
Final core receipt: `falcon-lab/.workflow/test-ryqTJC/receipt.json`, including
three new ingest tests and 29 Falcon tests after hoisting reference-path baking.
The TypeSpec registry classifies this extracted, tested scope at 2.7. Shared
content-type generation from TSP remains part of the carried P1 boundary work.

Promotion destination is independent of the task's state above:

| Tasks | Destination | Split to retain |
| --- | --- | --- |
| A1 | mixed | Shared code -> `crates/`; executable composition -> `smash/` |
| Q1 | library | Qualified reusable math integration and fixtures -> `crates/`; ruleset-specific expectations stay identified |
| Q2 | mixed | Bone/attachment/collision machinery -> `crates/`; Falcon scenario/content selection -> `smash/` |
| A2, A3 | mixed | Reusable input/state machinery -> `crates/`; Falcon routing and move policy -> `smash/` |
| Q3 | mixed | Replay/performance harness -> `crates/`; match scenarios and app transport wiring -> `smash/` |
| Q4, Q5 | mixed | Item/occupancy/destruction/support capabilities -> `crates/`; ship/item/stage definitions and game policy -> `smash/` |
| Q6 | classify per candidate | Every selected lab declares library/app portions before implementation |
| M1 | app | Match composition and its integration tests -> `smash/`; discovered shared portions get explicit library destinations |

Lab code remains eligible for promotion even when the destination is the app.
Fixtures, recordings and source provenance remain linked to promoted consumers.

## Candidate families

- Rust combat/input/state: ssbm_utils, ssbm-data, canon_collision, pf_sandbox_lib,
  FightersParadise fp-input/fp-combat/fp-vm. License and coupling gates apply.
- Formats/bones: brawllib_rs/rukaidata, ssbh_lib/ssbh_data/ssbh_wgpu,
  canon_collision's hurtbox/attachment tooling.
- Replay/SQL: peppi/peppi-slp/slippi-db; extract observations as fixture inputs,
  without treating replay telemetry as complete animation or behavior definitions.
- GDScript and JS/TS: existing v2 interpreter, v3 engine and v1 behavior sources;
  further Smash utility candidates enter this inventory with concrete overlap.
- Blender/other-language offline tools retain a separate, explicitly scoped
  ingestion role. No new unbounded tooling project is authorized by this queue.

Every lab records source version/license, exact API, target ruleset, fixture
provenance, executed assertions, mismatches, native/WASM coverage, timings where
relevant, reusable destination and the next coordinator checkpoint. A blocked
fixture ends that bounded lab with the missing evidence identified.
