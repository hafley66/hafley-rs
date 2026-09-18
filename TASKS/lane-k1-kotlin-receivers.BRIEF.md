# Lane K1: kotlin phase-1 receiver plane and the `receiver` leg

Issue: `issues/k1-kotlin-receiver-plane/item.md` (`issuectl show k1-kotlin-receiver-plane`). Epic `extract-parity-move-rename`.

You work in `$PWD`, your own worktree. Never `cd` anywhere outside it. The crate is `crates/sprefa-extract`, its own cargo workspace.

## Setup

- `export CARGO_TARGET_DIR=$PWD/crates/sprefa-extract/target-laned` before any cargo command.
- Run only ONE cargo process at a time. Never run the full gate in parallel with a build. The machine has 16 GB; a second concurrent cargo kills the session.
- Gate: `cd crates/sprefa-extract && cargo test --features cli --no-fail-fast`. Green at your base sha (179 targets).
- Read, in order: `crates/sprefa-extract/AGENTS.md` (the boundary law and the dl8 division of labor), `plans/2026-09-17-fast-slow-parity-and-move-rename.md` section "Lane C", `plans/reviews/2026-09-17-lane-c-rust-REPORT.md`, `plans/reviews/2026-09-18-lane-d-untyped-REPORT.md`.
- Build the binary: `cargo build --features cli --bin extract`; it lands at `$CARGO_TARGET_DIR/debug/extract`.
- Scope: kotlin only. Do not touch rust, ts, go, python files.

## Files you own

- `crates/sprefa-extract/src/lang/kotlin.rs`
- `crates/sprefa-extract/src/lang/kotlin_receivers.rs` (new)
- `crates/sprefa-extract/src/lang/kotlin_modules.rs`
- `crates/sprefa-extract/src/lang/mod.rs` (the one `mod kotlin_receivers;` line only)
- `crates/sprefa-extract/tests/137_kotlin_receiver_legs.rs` (new)
- `crates/sprefa-extract/tests/fixtures/kotlin_receivers/**` (new; NEVER under `tests/fixtures/kotlin/`, that is a ratchet corpus)
- goldens under `tests/fixtures/resolve/4_kotlin_resolved_edges.jsonl`, `tests/fixtures/kotlin/sample.v5.jsonl`, `tests/fixtures/kind_vocab/wire_golden.jsonl` (regen only, when a test's panic names them; say which in the report)

Not yours: `src/types.rs` (the row types you need exist: `ReceiverBinding`, `ReceiverOutcome::{Named, Inferred, Ambiguous, Shadowed}`, `MethodOwner` at `types.rs:641-660`), `tests/golden_parity.rs`, `tests/RATCHET.tsv`.

## The defect

`src/lang/kotlin.rs:1898` says it: "Name-only resolution ... (no receiver typing)". `Resolve<CallF> for KotlinSource` (`kotlin.rs:1733`) tries `module_target` then `call_name_match` for every site, receiver or not. `call.aux.receivers` is empty for kotlin; `call.aux.method_owners` is empty for kotlin. A navigation call `w.run()` binds to whichever corpus `run` is unique, or to nothing.

## The rule

Phase 1 mints one `ReceiverBinding` per navigation-call site (`recv.m(...)`, the `navigation_expression` under `call_expression`, see the call arm at `kotlin.rs:979-1000` and `:1184-1200`). Resolve adds a `receiver` leg ahead of `module_target`: a `Named(T)` receiver binds `m` to the member `m` declared in class/object/interface `T` in the corpus, origin `ResolutionOrigin::Receiver`. A `Named(T)` receiver with no such member binds nothing (a std or external type). `Inferred`, `Ambiguous`, `Shadowed` sites fall through to the existing legs unchanged in this lane (K2 changes that).

### K1.1 phase 1: `MethodOwner` rows

For every `function_declaration` and `property_declaration` directly under a `class_body` (of `class_declaration`, `object_declaration`, `companion_object`, interface), push `MethodOwner { span: <the def node span>, self_type: Some(<owner type_identifier>), trait_name: None }` to `sink.aux.method_owners`. The class walk already finds the owner at `kotlin.rs:141-165` and `:277-290`. Companion object members: `self_type` = the enclosing class name (calls are `Outer.m()`).

### K1.2 phase 1: the receiver walk (`kotlin_receivers.rs`)

Shape: `go_collect_receivers` at `src/lang/go.rs:1846-1900` (tree-sitter, one scope stack per function body). Template for the scope and outcome logic: `src/lang/rust_receivers.rs` (`TypeBinding`, `lookup`, closure scopes, `visit_expr_call` Shadowed rows).

Scope bindings, innermost wins, one frame per `function_declaration` body, lambda literal, and `class_body`:
- `parameter` / `class_parameter` with a `user_type` -> `Named(T)` (T = the `type_identifier`, generic args stripped; `nullable_type` unwrapped).
- `property_declaration` `val x: T = ...` / `var x: T` -> `Named(T)`.
- `val x = T(...)` where the initializer is a `call_expression` whose callee is an uppercase `simple_identifier` -> `Named(T)` (constructor call).
- `val x = f(...)` any other initializer -> `Inferred`.
- a type parameter `<P : Proj>` on the enclosing function -> a param `p: P` binds `Named(Proj)`.
- `this` inside a `class_body` / `object_declaration` -> `Named(<owner>)`; `this` outside -> `Inferred`.
- two conflicting `Named` declarations of one name in one frame -> `Ambiguous`.

Per site `recv.m(...)`:
- recv is a `simple_identifier` -> its binding, else `Inferred` when unbound and lowercase, else `Named(recv)` when unbound and uppercase (an object or companion spelled by name: `Gadget.spin()`).
- recv is `this` -> as above.
- recv is `a.f` (a `navigation_expression`) where `a` binds `Named(A)` and `A`'s `property_declaration` / `class_parameter` `f` has a written type `F` in the corpus -> `Named(F)`; that lookup happens in resolve, so phase 1 records `Inferred` here and resolve upgrades it through `MethodOwner` + the property's written type. Keep it to one hop.
- any other receiver expression (call result, literal, index, string template) -> `Inferred`.

Plain call `m(...)` where `m` is a scope-bound name -> push `ReceiverOutcome::Shadowed` at the callee span (rust_receivers.rs `visit_expr_call` is the model). Then delete `KotlinSource::shadowed` (`kotlin.rs`, the df-plane scan) and read the `Shadowed` row instead; the df dependence is a defect (off under `--family call`).

Every navigation-call site gets a row: the invariant lane D relies on is "no row means free call".

### K1.3 resolve: the `receiver` leg

In `Resolve<CallF> for KotlinSource`, before `module_target`: look up the site's `ReceiverBinding`; on `Named(T)` find the corpus def `m` whose `MethodOwner.self_type == T` (search every blob's `call.aux.method_owners` joined to its def nodes by span, through `cx.indexes.def_index` + `corpus_defs(index, callee)`; the go leg `go_method_on_type` at `go.rs:3721` is the shape). Prefer the def in the file the module plane binds `T` to (`KtModuleIndex::import_target` / `package_scope`), else the unique corpus owner; two owners -> no edge. Origin `ResolutionOrigin::Receiver`. A `Named(T)` with no member hit -> `continue` (no fall-through to name match). Interface members count as members of the interface name.

## Verification, in order

1. Fixtures first, `tests/fixtures/kotlin_receivers/`:
   - `lib.kt`: `package acme` ; `class Widget(val id: Int) { fun run(): Int = id }` ; `class Holder(val w: Widget)` ; `object Gadget { fun spin(): Int = 1 }` ; `interface Proj { fun project(): Int }` ; `class Decoy { fun run(): Int = 2 }` ; `fun makeWidget(): Widget = Widget(1)`.
   - `use.kt`: `package acme` ; `fun paramLeg(w: Widget) = w.run()` ; `fun ctorLeg() { val w = Widget(2); w.run() }` ; `fun returnLeg() { val w = makeWidget(); w.run() }` ; `fun fieldLeg(h: Holder) = h.w.run()` ; `fun boundLeg<P : Proj>(p: P) = p.project()` ; `fun objectLeg() = Gadget.spin()` ; `class Inner(val id: Int) { fun me() = this.id ; fun self() = run() ; fun run() = 0 }` ; `fun shadow(run: () -> Int) = run()`.
   - Expected, hand-derived: `paramLeg`, `ctorLeg`, `fieldLeg`, `boundLeg`, `objectLeg` -> origin `receiver`, callee in `lib.kt`, and `run` binds `Widget.run` NOT `Decoy.run` (assert `callee_start` equals the span of `Widget`'s `run`, take it from `--family call` def rows). `returnLeg` -> `unresolved` reason `inferred` in K1 (the ctor-return leg through a fn's declared return type is optional; if you add it, say so). `shadow` -> zero edges to any `run`, reason `inferred`. `Inner.self()` -> `run` binds `Inner.run` origin `receiver` (implicit this).
2. `cargo test --features cli --test 137_kotlin_receiver_legs --test 131_kotlin_module_resolve --test 90_mutation_battery --test golden_parity`.
3. CTF: from `crates/sprefa-extract`, `$CARGO_TARGET_DIR/debug/extract fast --sqlite $CARGO_TARGET_DIR/ctf.db tests/fixtures/kotlin/*.kt tests/fixtures/kotlin_module_resolve/**/*.kt tests/fixtures/kotlin_receivers/*.kt` then `SELECT resolution_origin, COUNT(*) FROM resolved_edge GROUP BY 1` and `SELECT reason, COUNT(*) FROM unresolved GROUP BY 1` at base and at HEAD, verbatim in the report.
4. Full gate. A test that fails because it pinned a name-guessed kotlin member call gets its expectation changed with a one-line comment naming lane K1; a test that fails for any other reason stops you: `## Blocked`.

## Commits

Subjects exactly:

- `feat(extract): kotlin phase 1 mints method owners and receiver bindings`
- `feat(extract): kotlin receiver leg binds members through the corpus owner table`
- `fix(extract): kotlin shadowing reads the receiver plane, not df`

Commit trailer on each: `Refs-Issue: @k1-kotlin-receiver-plane`. Do not push.

## Deliverable

`REPORT.md` at the worktree root. Tables only. Sections:

1. `## Commits` : sha, subject, files.
2. `## Legs` : one row per leg in the fixture (param, ctor, return, field, bound, object, this, shadow): expected, observed, origin.
3. `## CTF` : the two query outputs at base and at HEAD, verbatim.
4. `## Tests changed` : test name, old expectation, new expectation, why the old one encoded a guess.
5. `## Gate` : last 3 lines.
6. `## Blocked` : empty, or the exact error and the diff you wanted.

## Laws

- Stop and write `## Blocked` when a command fails in a way this brief did not anticipate.
- No em dashes. No praise. Facts and receipts.
- Rust comments: at most 2 consecutive comment lines; state only constraints the code cannot show.
- Tests are integration tests through the real `extract` binary on fixture files. No mocks, no fakes. Expected values are hand-derived from the fixture, never copied from the extractor's output.
- Do not build a watcher, daemon, delta resolver, or persistent index (AGENTS.md, dl8 division of labor).
