# Living structural timeline

[D2 source](1_roadmap.d2) · [Rendered map](1_roadmap.svg) · [Tasks](3_tasks.md)

The top inventory group is now generated from the
[checked TypeSpec registry](classification/1_registry.tsp). `just status` resolves
Cargo references; `just test` rejects malformed/stale records. See
[classification workflow](classification/0_readme.md) for stage definitions and
limits. Historical DONE colors continue to describe scoped proofs independently
of the inventory's numbered promotion stages.

Run `just map`, `just test-map`, or `just map-watch` from `games/`.
D2 0.7.1 and ELK are the checked renderer. The map skill supplied the stacked-grid
layout and adjacent-milestone edge convention. New milestones go above `m4`, under
the legend. Stable IDs identify capabilities; status changes do not rename IDs.
Date of this inventory: 2026-09-10. DONE means the named scoped proof exists;
historical tests have not all been rerun for this documentation change.

## Lab maturity and promotion destination

`lab` means pending promotion. Destination is a separate axis:

| Destination | Promoted location | Examples |
| --- | --- | --- |
| library | `crates/` | Collision/animation utilities, reducers, input machinery, rollback, SQLite adapters, codegen tooling, capture and reusable test harnesses |
| app | `smash/` | Falcon behavior/content selection, Small Battlefield setup, game-specific control policy, match composition and integration scenarios |
| mixed | Both, explicitly separated | Bone lab: reusable attachment evaluation plus Falcon fixtures; ship lab: reusable occupancy/control machinery plus ship definition |

Passing a lab establishes its stated evidence scope. Promotion also moves its
implementation/tests and updates consumers. An app-specific lab can be promoted
directly into the app. Keep evidence linked; no assumption that lab work gets
discarded. Destination labels and maturity colors must remain independent as the
map grows. Current task classifications are below; individual source splits are
verified before moving code.

## Evidence index

| Map group | Source / receipt | Established scope and remaining work |
| --- | --- | --- |
| m0.v1 / m0.v2 | [Sibling archive](../../smashy/reference/og-v1/), specifically `gdscript-labs/v2-godot-language-lab/README.md.txt` and its interpreter | v1 gameplay priorities are user-confirmed. Archived v2 source documents vehicle, attachment, input and destruction rows; executability not checked here. |
| m0.v3 / m0.v4 / m0.dom | User's version history; `smashy/reference/og-v1/gdscript-labs/v3-game-dom-css-lab/` in sibling repository | v3 behavior is an extraction source. Rust v4 and document/DOM direction are retired. |
| m1.content | [Importer](blender-godot-sqlite-proof/falcon-lab/2_main.rs), [first recording](blender-godot-sqlite-proof/falcon-lab/7_readme.md) | Cached HTML payload, base64/bincode, brawllib_rs HighLevelSubaction. Per-frame bone transforms place hurtbox capsules. This is evaluated frame data, not complete executable PM behavior. |
| m1.hit | [Launch proof](blender-godot-sqlite-proof/falcon-lab/20_launch_readme.md), [sandbag](blender-godot-sqlite-proof/simulation-core/1_sandbag.rs) | Parry intersection, ssbm_utils formula calls, Rapier target flight. Weight-100 target and Melee formulas need explicit PM qualification. |
| m1.output / m2.boundary | [SQL proof](blender-godot-sqlite-proof/falcon-lab/26_sql_readme.md), [Godot proof](blender-godot-sqlite-proof/falcon-lab/33_godot_readme.md), [faults](blender-godot-sqlite-proof/falcon-lab/63_fault_readme.md), `falcon-lab/contracts/0_presentation.tsp` | Existing row/payload/codegen and adapter proofs; reusable extraction remains. |
| m2.state / m2.rollback / m2.capture | [Shared provenance](shared/0_reuse.md), [latest lab receipt](blender-godot-sqlite-proof/falcon-lab/109_tasks.md) | Shared reducers, statig buffer, GGRS proof harness, capture. Historical scoped tests/MP4 and WASM compile pass; full cross-target match is unverified. |
| m2.simulation | [World and step](blender-godot-sqlite-proof/simulation-core/2_simulation.rs) | Seven-action loader, mutable World, shared immutable actions. Current movement is lab policy. |
| m2.donor | Sibling `hafley-rs-game-runtime/games/cascade/` and `games/physics2d/` | Typed rule resolution, contact/support mechanics exist. Extraction and qualification remain. |
| m3 | [Domain inventory](blender-godot-sqlite-proof/research/10_smash_ecosystem.md) | Candidate inventory is broader than executed labs. Q1–Q6 track qualification without claiming completion. |
| m4 | [App destination](smash/0_readme.md), [crate destination](crates/0_readme.md) | Approved paths. Package creation, moves and app integration remain A1. |
| m6 | [Ledger](4_tasks.md) rows S7–S10; receipts `a5ce0af`, `fc11379`, `6bb2d0f` | Foundations integrated. Four uncommitted lanes remain: browser simultaneous-direction input (S7), bounded-rotation atomic-consume trace backend (S8), readable runtime statechart UI (S9), mocked real-harness backbone (S10). No SHA, no stage change. |
| m7.router | [Registry](classification/1_registry.tsp) `game-ui`; sibling runtime `games/kneeman/app/crates/godot-shell/src/ui/menu/router.rs` and `menu/mod.rs` | Proposed library lift of the proven `Location`/`Nav`/`NavCmd` memory router. The reducer is pure and snapshot-safe; URI codec, dialog gate and host-neutral view projection are specified. No crate exists at stage 1. |
| m7.adapters | `blender-godot-sqlite-proof/falcon-lab/godot/2_input.gd`, `2_stage.gd`, `3_control.test.gd`; S7 `f5b1142`/`98b4631` | Observed adapters: keys with last-pressed resolution, first connected generic joypad, web touch bar Left/Right/Jump/Fair/Reset/Proof. GameCube-specific adapter absent; hardware unmeasured. GameCube-proportioned touch layout is reference only in sibling `kneeman-lines`. |
| m7.deploy | `blender-godot-sqlite-proof/falcon-lab/.workflow/deploy.json` (`a5ce0af`, source `428fe2e0`), `prove.json` (`98b4631`, source `addc7e5c`) | `/game3/` serves the older deploy receipt with protected `/game/` unchanged. Current source has no matching receipt and is stale after later changes. Rollback is qualified in isolation; live native-vs-WASM match/netplay remains unqualified. |
| m8.origin | [Manifest](smash/src/fighters/falcon/imported/0_sources.json) `boundary` | Rukaidata GitHub is generator/parser source; retained PM3.6 files are literal generated webpage artifacts containing base64+bincode; raw PAC/GCT inputs are absent. Melee transition rules come from the pinned `doldecomp/melee` submodule `games/vendor/melee`. |
| m8.rules | [Generated rules](smash/src/fighters/falcon/generated/2_source_rules.json), `61b2195`, `just source-rules-check` | Five guards from `ftCo_Turn.c`, `ftCo_Jump.c` and `ftCo_JumpAerial.c` at revision `c7861544`, each with a repo/revision/path/line `SourceRef`, rendered to D2/SVG. `just status` prints the SOURCE RULES section. No stage advances. |
| m8.open | `generated/2_source_rules.json` `unresolved` | Three values stay explicit: `p_ftCommonData->x34` and `p_ftCommonData->x78` (numeric values in the game common-data binary; no retained DAT input) and the PM3.6 `LandingLight` selection rule. Source fidelity remains unqualified. |
| m9.translator | `crates/content/src/5_source.rs`; `tree-sitter` 0.24.7 and `tree-sitter-c` 0.23.4 (MIT) Cargo pins | Bounded lowering over the existing `game-content` `ingest` parser. Supported nodes: parentheses, unary `-`, binary `*`, the six comparisons, `fp->input.lstick[0].x`, `fp->facing_dir`, `p_ftCommonData-><field>` and `ftCo_MS_<action>`; every other expression returns `SourceError::UnsupportedExpression`. No new parser or general C compiler. |
| m9.port | Generated `crates/ftcommon/src/generated/0_ftcommon.rs`; tests `crates/ftcommon/tests/1_source_rules.rs` | `game-ftcommon` owns the typed boundary (`FighterQuery`, `CommonData`, `FtMotionId`); `ftCo_800C97A8`, `ftCo_Jump_Enter` and `ftCo_JumpAerial_Enter_Basic` are generated with repository/revision/path/line-span, the C names, the source expression and the untranslated call list. Four fixtures evaluate every emitted rule, including the facing sign flip and a non-zero `x78`, against the generated Rust. |
| m9.open | Generated provenance comments; `4_tasks.md` S11 | `p_ftCommonData->x34`/`x78` remain typed inputs, never literals. Untranslated calls (`ftCommon_8007D5D4`, `Fighter_ChangeMotionState`, `ftCo_800CB110`, `ftCo_800CBAC4`, `PAD_STACK`) are recorded but not emulated, and nothing in the live simulation consumes `game-ftcommon` yet. Stage 2.7 on the draft/testing exit, no promotion. |

## Runtime ownership and rollback

| Data | Lifetime / storage | Restore rule |
| --- | --- | --- |
| Skeleton, bind pose, baked animation frames, attack definitions, item/chart definitions | Immutable content loaded before play; shared across snapshots | Retain identical content identity/version/hash across replay and peers. |
| Current action, frame/fraction, playback rate, hitlag, transition timers, input history, facing, damage | Authoritative Rust entity/world state | Snapshot and restore every value that influences future gameplay. |
| Item instances, holder/pilot relationships, active modifiers, expiration, RNG, stage destruction | Authoritative Rust entity/world state | Snapshot instances and activation state; definitions remain shared. |
| Dynamic bone/procedural pose state that affects collision | Authoritative state or proven deterministic derivation | Restore its inputs/state; Q2 must establish equivalence. |
| Bone matrices/capsules derived entirely from content + restored state | Reusable scratch/cache | Rebuild exactly. No need to copy immutable matrices each tick. |
| Rapier durable body/collider/joint and other causally relevant state | Physics portion of authoritative world | Restore required state. Rebuilding caches requires replay tests; do not assume transforms alone suffice. |
| SQLite presentation rows / GPU buffers / cosmetic interpolation | Output adapters | Republish corrected output after replay catch-up. Do not use stale presentation rows as gameplay authority. |
| Local shell route, dialog gate, nav history, renderer focus/hover/layout | Host-local Redux/UI state, outside the gameplay snapshot | Never restored by gameplay rollback. Match/rules/item commands cross as authoritative effects/events; the route is a statechart codec, not a second source of truth. |

Redux supplies state-transition composition. It does not require an action per bone
or a serialized mesh per tick. Existing reducers mutate their owned World. GGRS
requests save/load/advance through the shared rollback adapter. SQLite exposes
published state for query/render/inspection, and TSP defines shared boundary shapes.

`cgmath` is matrix/vector math used by the imported brawllib representation and
bone transforms. Parry supplies geometric shapes/intersections; Rapier supplies
physics stepping and uses collision machinery. They are different responsibilities.
Whether the importer-side cgmath representation is converted at bake time is an
integration decision; do not propagate it through every shared API by default.

## First behavior gate

Latest requested scope: walk, jump, crouch, directional attack/special triggering,
side smash, down tilt, neutral air, fair and bair. "Crouch tilt" provisionally means
down tilt. Input routing uses grounded/airborne state, facing, stick direction and
timing. Side-smash versus tilt thresholds are ruleset data requiring PM evidence.
Special routing and the implementation of each special are separate coverage rows.
No automatic claim that every Falcon special is available.

Authored attack timing and poses come from offline imports. Gameplay chooses the
action/frame and evaluates movement/combat rules. Bone matrices place hurtboxes
and attachments. Parry queries candidate contacts; game rules choose damage/launch;
Rapier advances configured physical bodies. Exact character movement and stage
response remain qualification work. cgmath does not implement physical response.

Performance gate: ordinary frame and rollback-burst timings, allocations and
entity/destruction stress on named hardware. 16.67 ms is the 60 Hz frame deadline,
not an established simulation allowance. Content decoding stays off the hot path.
