# Redux boundary audit

Revision audited: `4a562b52549d0c5072d68b75df28ee2376bbdf44` (merged main).
Scope: every tracked file under `crates/redux`. Question: do physics, game,
or application assumptions leak into the reusable Redux library, and where do
the domain-bearing examples belong.

## Contents

1. Verdict
2. Production dependency graph
3. Production source census
4. Public surface
5. Examples and destination
6. `_4_machine_macro.rs` promotion audit
7. Tests
8. Out-of-scope consumer gaps
9. Edits and validation

## 1. Verdict

No production coupling found. The library (`src/`, manifest normal deps) names
no phase, fighter, body, physics, render, game-tick, or application-state field
and requires none. Every domain reference lives in `examples/`, `tests/`, or
dev-dependencies. Documentation in `0_slice.rs`, `2_effects.rs`, and `lib.rs`
framed `Slice` around phase and game-tick vocabulary or stated enforcement the
trait does not provide; those lines were corrected to describe the intended
contract. No API, feature, dependency, or structural change was made.

## 2. Production dependency graph

Normal dependencies (`[dependencies]`, `Cargo.toml:10-12`):

| dep | optional | role |
| --- | --- | --- |
| `serde` (derive) | yes, feature `serde` | snapshot derives |
| `serde-big-array` | yes, feature `serde` | arrays past serde's len-32 impls |

No game crate appears in the production graph.

Dev-only dependencies (`[dev-dependencies]`, `Cargo.toml:17-24`): `serde`,
`serde_json`, `tokio`, `tempfile`, `tracing`, `statig =0.4.1` (`serde`, not
`macro`), `rollback` (path), `ggrs =0.13.0`. These reach examples and tests
only.

Dev-cycle: `redux` dev-depends on `rollback`, and `rollback` depends on
`redux` at `crates/rollback/Cargo.toml:12`. The cycle exists only for
integration testing through the real rollback harness. It does not enter the
production graph and does not pull ggrs, statig, tokio, or tempfile into a
library build.

## 3. Production source census

Keyword occurrences in `crates/redux/src/*.rs` after the edits in section 9:

| keyword | count | locations | production requirement |
| --- | --- | --- | --- |
| `phase` | 0 | | none |
| `tick` | 0 | | none |
| `fighter` | 0 | | none |
| `physics` | 0 | | none |
| `jump` | 0 | | none |
| `rapier` / `statig` / `gameplay` | 0 | | none |
| `render` | 1 | `lib.rs:6` parenthetical subscriber example | none |
| `rollback` | 5 | `lib.rs:15,16,19`; `1_pool.rs:12,256` | none |

`body` occurrences are macro metavariables `$body:block`, not a physics body.

Dispositions:

| file:line | text | disposition |
| --- | --- | --- |
| `lib.rs:6` | `(a render pass, a logger, a persistence hook)` | keep: one item in a subscriber list, not a requirement |
| `lib.rs:11-28` | rollback netcode/session framing | keep: names `crates/rollback`, a real consumer, as motivation |
| `1_pool.rs:12,256` | "rollback invariant" | keep: names a determinism property, no field or phase bound |

No struct, trait, function, or macro in `src/` stores or requires a phase,
game tick, body, or application field.

## 4. Public surface

Re-exported at `src/lib.rs:65-67` and root: `Slice`, `Then`, `MapEffect`,
`Lens`, `Zoom`, `Each`, `EachCx`, `Never`, `slice!`, `reduce_then_apply`,
`FreeSpans`, `Handle`, `Pool`, `Store`, `Sub`, `replay`.

| item | location | domain assumption |
| --- | --- | --- |
| `Slice` (+ `Context`/`State`/`Event`/`Output`/`Effect`) | `0_slice.rs:11-23` | none; caller owns all types |
| `Never` | `0_slice.rs:26` | none; uninhabited effect |
| `Then`, `MapEffect`, `Zoom`, `Each` | `0_slice.rs:29-110,203-237` | none; composition only |
| `slice!` | `0_slice.rs:113-155` | none; no phase arm, effect defaults to `Never` |
| `reduce_then_apply` | `2_effects.rs:12-29` | none |
| `FreeSpans`, `Handle`, `Pool` | `1_pool.rs` | none |
| `Store`, `Sub`, `replay` | `lib.rs:69-140` | none |

`Slice`, `Then`, `Zoom`, `Each`, and the effect composition remain the generic
algebra every consumer builds on; all were inspected and none needs a phase or
game field.

## 5. Examples and destination

Examples build with dev-dependencies. `cargo run --example` and
`cargo test --example` build one directly; `--all-targets` builds them together
with the tests. The manifest places every example-only crate (`tokio`,
`tempfile`, `tracing`, `statig`, `rollback`, `ggrs`) under
`[dev-dependencies]`, so a plain library build never compiles them.

| file | domain content | destination classification | action |
| --- | --- | --- | --- |
| `examples/1_asset_loop.rs` | Tokio file-read effect + bounded host | library pattern; reusable by an assets host | keep; move would need parent approval, not requested |
| `examples/_2_hierarchical_slice.rs` | statig hierarchy, network state names, `crates/fighter` reference in prose | library qualification of statig; names are illustrative |
| `examples/_3_shared_locomotion.rs` | grounded/airborne machine, physics intent descriptors | game-domain demo of `Then`/`Zoom` |
| `examples/_4_machine_macro.rs` | locomotion machine + menu machine | game-domain demo of `slice!` lowering |
| `examples/_3_machine_contract.md` | statig/fighter/physics contract | lab contract document |

Domain examples are not production leakage. Names alone are not grounds to
delete or relocate; no move is proposed in this cut.

## 6. `_4_machine_macro.rs` promotion audit

Status: example-only. The file owns phase itself through a caller state field;
it is not a library feature and was not promoted or redesigned.

Observed syntax restrictions, as the macro is actually written
(`_4_machine_macro.rs:95-135`). Each is a fact about the current text, not a
defect:

| observed restriction | syntax |
| --- | --- |
| exactly one phase field on the caller state | `phase: $phase:ident via $field:ident`; dispatch is `match (st.$field, ev)` |
| state and event variants are unit variants | `<phase>::$src`, `<$ev>::$evt` |
| output is a two-variant enum, one handled and one unhandled | `output: $out:ty { handled: $hand:ident, unhandled: $un:ident }` |
| guards and actions are free functions with fixed signatures | `fn(&State, Cx) -> bool`, `fn(&mut State, Cx, &mut impl FnMut(Effect))` |
| one effect type per machine | a single `$eff:ty` |
| one `Copy` context | `context: $cx:ty`; this matches the existing `Slice` bound `Context<'a>: Copy` (`0_slice.rs:12`) |
| node list declared apart from the rows | `states: [...]` plus row sources and targets |
| flat `match`, no hierarchy | superstates are absent |
| a transition carries no transition kind | equal source and target variants are indistinguishable |

Optional proposals, only if a promotion is ever requested. None is a
prerequisite for the current example:

| optional proposal | note |
| --- | --- |
| let the chart state be the `Slice::State` itself or be selected by a `Lens` | the `via $field` restriction is the only reason a wrapper field is needed |
| accept payload events | current examples use unit events only |
| carry richer outcomes | a caller can wrap the two-variant output or map it in a `Then` layer; no separate `Handled` trait is needed |
| widen the effect type | choose a wider enum as `$eff`, or apply the existing `MapEffect` (`0_slice.rs:59-80`); the library already provides this |
| allow guard/action methods or closures | current free-function form is sufficient for the examples |
| derive the node list from the rows | optional; rows name only edge endpoints, so a row-only derivation would omit an isolated state that has no edges |
| add self-vs-distinct transition kind | optional; only needed when a clock or lifecycle hook differs between them |
| add hierarchy | statig covers this in `_2_hierarchical_slice.rs`; out of scope for `machine!` |

Validation, by kind. Compile-time checks are only those the code emits;
everything else below is runtime example testing and is not a theorem:

| check | kind | evidence |
| --- | --- | --- |
| generated slice is zero-sized | compile-time | `slice!` emits `const _: () = assert!(size_of == 0)` (`0_slice.rs:138`) |
| node list and row endpoints agree | compile-time linkage absent; runtime endpoint coverage tested | no compile-time linkage between `states` and the rows; `loco_graph_ids_are_unique_and_endpoints_declared` and `menu_graph_ids_are_unique_and_endpoints_declared` check endpoint coverage at runtime |
| guard/action bodies are pure | not checked | ordinary Rust callbacks; no purity check |
| generated dispatch matches the handwritten oracle over tapes | runtime example test | `machine_matches_handwritten_oracle_over_tapes` |
| graph node and edge literals are stable | runtime example test | `loco_graph_nodes_and_edges_are_stable`, `menu_graph_nodes_and_edges_are_stable` |
| graph ids are unique and endpoints declared | runtime example test | `loco_graph_ids_are_unique_and_endpoints_declared`, `menu_graph_ids_are_unique_and_endpoints_declared` |
| every declared edge fires | runtime example test | `every_declared_loco_edge_fires`, `every_declared_menu_edge_fires` |
| `Then` defers only unhandled events | runtime example test | `then_composition_defers_only_unhandled_events`; the `Then` seam itself is `0_slice.rs:31-56` |
| clone and serde restore equal a fresh run | runtime example test | `clone_and_serde_suffix_restoration_equals_reference`, `menu_serde_restore_equals_reference` |

The graph and the dispatch body both expand from the same rows in one macro
invocation, which is why the graph tests can compare them. That is a property of
this expansion, not a compile-time guarantee that the declared `states` list
covers every row endpoint. The graph and `Then` tests above were merged with the
example in commit `8574b15` (`redux: prove Then composition and graph shape in
machine-macro lab`), the row-lowering proof in `35c1ca4`; both are present at
the audited revision.

## 7. Tests

| file | kind | external surface |
| --- | --- | --- |
| `tests/0_slice_external.rs` | pure computation | public `slice!`/`Then`/`Zoom`/`Lens`, `Never` |
| `tests/3_effects.rs` | pure computation | `reduce_then_apply`, effect ordering and buffer reuse |
| `tests/2_statechart.rs` | integration | dev `statig`, `rollback`, `ggrs`; gated on feature `serde` |
| `tests/4_asset_loop.rs` | integration | real Tokio runtime, `tokio::fs`, tempfile; shares `examples/1_asset_loop.rs` |

The I/O test uses real files and a real runtime; no executor or filesystem
fake. The statechart test drives the real ggrs sync-test session. No test
assertion or effect order was changed.

## 8. Out-of-scope consumer gaps

Production consumers of `redux`, outside this lane's ownership:

| consumer | manifest | note |
| --- | --- | --- |
| `smash` | `smash/Cargo.toml:25` | app crate |
| `crates/ui` | `crates/ui/Cargo.toml:12` | uses `Slice`, `Never` |
| `crates/fighter` | `crates/fighter/Cargo.toml:12` | feature `serde` |
| `crates/vehicle` | `crates/vehicle/Cargo.toml:11` | |
| `crates/physics` | `crates/physics/Cargo.toml:11` | |
| `crates/input` | `crates/input/Cargo.toml:9` | |
| `crates/rollback` | `crates/rollback/Cargo.toml:12` | also a dev-dep of `redux` |
| `blender-godot-sqlite-proof/wasm-port-lab` | lab manifest:12 | lab |

Gap: WASM (`wasm32`) compilation of `redux` was not executed in this lane;
`Pool` and `FreeSpans` use no heap and should port, but that is unverified here.
The consumer crates were read only for their dependency edge; their internals
were not audited.

## 9. Edits and validation

Public behavior unchanged; documentation only.

| file:line | before | after |
| --- | --- | --- |
| `src/0_slice.rs:1-6` | "one shape for phases, screens, and systems"; "Effects escape only as inert descriptors"; "all mutable state remains in the caller-owned `State`"; "monomorphize to straight-line code" | generic `(state, event, context) -> output` shape; "a convention, not an enforced boundary"; an implementer can perform I/O, mutate external state, or emit effects the caller settles; "Compositions are zero-sized types, which lets pipelines monomorphize" |
| `src/0_slice.rs:10-11` | "One reducer / one phase"; "same-tick"; "runner outside the snapshot state" | "One reducer step"; "the same dispatch"; effects received "synchronously inside `reduce`, with settlement timing (immediate or deferred) chosen by the caller" |
| `src/2_effects.rs:8` | "reused by the next tick" | "reused by the next call" |
| `src/lib.rs:3-4` | "The reducer is a pure function" | "The reducer is intended to be a pure function" |

Validation, run with `CARGO_TARGET_DIR` set to the lane target and `-j1`:

| command | result |
| --- | --- |
| `cargo test --manifest-path crates/redux/Cargo.toml --locked --offline -j1 --all-targets` | pass: 15 unit, 1 external, 2 effects, 11 asset-loop, 10 + 10 + 17 example; 0 failed |
| `cargo test --manifest-path crates/redux/Cargo.toml --locked --offline -j1 --doc` | pass: 1 doctest |
| `cargo check --manifest-path crates/redux/Cargo.toml --locked --offline -j1 --no-default-features` | ok |
| `cargo check --manifest-path crates/redux/Cargo.toml --locked --offline -j1 --features serde` | ok |

Native only. WASM was not executed. One pre-existing warning in the merged
`examples/_4_machine_macro.rs:359` (`unused variable: deferred`) remains; it is
outside this change and was not touched. No public code changed, so the feature
checks guard the manifest, not behavior.
