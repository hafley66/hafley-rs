# Lane FR: flash-lane review cleanups, twelve rows

Issue: `issues/flash-review-cleanups/item.md` (`issuectl show flash-review-cleanups`). Epic `extract-parity-move-rename`.

You work in `$PWD`, your own worktree. Never `cd` anywhere outside it. The crate is `crates/sprefa-extract`, its own cargo workspace.

## Setup

- `export CARGO_TARGET_DIR=$PWD/crates/sprefa-extract/target-laned` before any cargo command.
- Run only ONE cargo process at a time. 16 GB machine.
- Gate: `cd crates/sprefa-extract && cargo test --features cli --no-fail-fast`. Green at your base sha.
- Read `crates/sprefa-extract/AGENTS.md` first, then only the receipts each row names.
- Scope: behavior-preserving. No new resolver legs, no new origins, no fixture text changes except where a row says so. `tests/RATCHET.tsv` stays byte-identical.

## Files you own

- `crates/sprefa-extract/tests/130_rust_spelled_receiver.rs`
- `crates/sprefa-extract/tests/134_ts_binding_legs.rs`
- `crates/sprefa-extract/tests/131_kotlin_module_resolve.rs`
- `crates/sprefa-extract/tests/golden_parity.rs` (the three ratchet fns only)
- `crates/sprefa-extract/src/lang/kotlin_modules.rs`
- `crates/sprefa-extract/src/lang/kotlin.rs` (`module_target` and its call sites only)
- `crates/sprefa-extract/src/lang/ts_receivers.rs`

Not yours: `src/lang/ts_rename.rs` (lane TF starts after you and reads `ts_receivers.rs`; keep every `pub(crate)` signature in `ts_receivers.rs` unchanged), `src/lang/rust_rename.rs` (lane E is live on it), `src/lang/kotlin_receivers.rs`, `src/types.rs`, any fixture under `tests/fixtures/` except where a row below names one.

## The rows

Each row: fix it, or close it in REPORT.md with one line saying why the code is right as it stands. A close needs a receipt (a file:line that shows the row's premise false).

| # | kind | receipt | what is wrong | fix |
| --- | --- | --- | --- | --- |
| 1 | weak test | `tests/130_rust_spelled_receiver.rs:101-108` `trait_bound_generic_receiver_binds` asserts `has_origin(rows, "trait_bound_leg", "run", "proj", "receiver")` | the fixture has `Proj::run` (trait) and `Widget::run` (impl) in different files; the assert names the file stem only, so a bind to any `run` in `proj.rs` passes | assert the `callee_start` byte offset of the trait fn's def, derived from the fixture bytes with a marker search (the shape `tests/137_kotlin_receiver_legs.rs` `def_start` uses) |
| 2 | weak test | `tests/134_ts_binding_legs.rs:131-175`: three shadow tests (`a_param_shadow_kills_the_name_match`, `a_const_binding_shadow_kills_the_name_match`, `an_arrow_param_shadow_kills_the_name_match`) all call `shadow_run()` and assert on the same `drops` vector without a caller key | any one shadow site dropping satisfies all three tests | key each assert on the site: `(caller_name or span, reason)`; one shadow site per test, the others must not be asserted by it |
| 3 | weak test | `tests/134_ts_binding_legs.rs`: `use.ts` `crossFileGeneric` has no test | a cross-file generic-bound bind is in the fixture and never asserted | add `the_generic_bound_leg_fires_cross_file` mirroring `the_ctor_return_leg_fires_cross_file` (`:89`) |
| 4 | weak test | `tests/131_kotlin_module_resolve.rs:147` `!calls.iter().any(callee == "dupName")` | passes when the module leg is off entirely (no edges at all means no `dupName` edge) | also assert a positive edge in the same run (`lone` at `:125` is in the same corpus: assert both in one test, or assert the `unresolved` row for `dupName` with reason `ambiguous`) |
| 5 | wrong comment | `tests/131_kotlin_module_resolve.rs:152-160` | the K1 lane rewrote this comment; verify it matches the fixture (`Gadget.spin()` in `main`); the review's original complaint was the pre-K1 comment calling `spin` "not a top-level decl the plane indexes" | if the K1 text is accurate, close with the receipt |
| 6 | paste x3 | `tests/golden_parity.rs:1205-1223` (ts), `:1560-1574` (go), `:1900-1915` (rust): the eprintln totals + `lang\torigin` header loop + `join_hits` assert + `pin_ratchet_tsv` call | three copies of the same 20 lines differing in the `"ts"`/`"go"`/`"rust"` literal | one `fn report_and_pin(lang: &str, tool: &str, total_sites: usize, counts: &RatchetCounts)`; the three ratchets call it; stderr output byte-identical to today (diff a captured run) |
| 7 | dup index | `src/lang/kotlin_modules.rs:111` `names_by_package: HashMap<(String, String), String>` built at `:142`, read by `package_scope` at `:158`; `declaring_file` at `:192` computes the same answer from `package_files` + `facts.top_level` | two sources of truth for "which file declares `name` in `package`" | drop `names_by_package`; `package_scope` calls `declaring_file`. Keep the ambiguity rule (`two files declaring it is ambiguous`) |
| 8 | Option plumbing | `src/lang/kotlin.rs:1826-1834` `module_target(modules: Option<&KtModuleIndex>, own_path: Option<&str>, index, paths: Option<&PathIndex>, ..)` then `let (modules, own_path, paths) = (modules?, own_path?, paths?);` | every caller already holds the Options; the fn unwraps them on the first line | take `&KtModuleIndex`, `&str`, `&PathIndex`; callers do the `?`/`zip` at the call site (`:1792` module leg and `:2039` in `call_drops`; `receiver_target` at `:1892` calls `module_type_file` at `:1921`, which already takes the unwrapped shape |
| 9 | parallel stack | `src/lang/ts_receivers.rs:212` `locals: Vec<HashSet<String>>` pushed/popped at `:430,:447,:452,:464` beside the scope frames; read at `:235,:512,:555` | a second stack that must stay in lockstep with the binding scopes | seed the binding scope with `Inferred` for every unannotated param / untyped local at the point `locals` inserts today; delete `locals`; the reads become "is the name bound in scope" lookups. Call-site row output byte-identical (test 134, 136, 40, golden_parity ts ratchet) |
| 10 | clears vs stacks | `src/lang/ts_receivers.rs:216-222` `load_type_params` does `self.type_param_constraint.clear()` | a nested function's type params wipe the enclosing fn's constraints for the rest of the outer body | push a frame on function entry, pop on exit, lookup walks frames innermost-first; add a fixture case in `tests/fixtures/ts_binding_legs/` with an inner generic fn followed by an outer-bound call, and assert it in 134 |
| 11 | weak test | `tests/130_rust_spelled_receiver.rs:122` `shadowed_call_does_not_bind_free_fn` | verify the drop assert is keyed to the `run` site, same defect shape as row 2 | key it, or close with the receipt |
| 12 | comment law | every file you touch | AGENTS.md comment law: at most 2 consecutive comment lines, only constraints the code cannot show | apply while there; no comment-only commits |

## Verification, in order

1. Row 6: capture `cargo test --features cli --test golden_parity call_resolve_scip_ratchet_ 2>stderr.before` at base, again after; `diff` of the `[ts-total]`/`[go-total]`/`[rust-total]` lines and the `lang\torigin` tables is empty.
2. `cargo test --features cli --test 130_rust_spelled_receiver --test 134_ts_binding_legs --test 131_kotlin_module_resolve --test 136_untyped_receiver_ts --test 40_ts_resolve --test 137_kotlin_receiver_legs --test golden_parity` (adjust names to `ls tests/`).
3. `tests/RATCHET.tsv` unchanged: `git diff --exit-code tests/RATCHET.tsv`.
4. Full gate.

## Commits

One commit per kind, subjects exactly:

- `test(extract): receiver-leg tests assert the def offset and key drops by site`
- `refactor(extract): golden_parity ratchet report and pin in one fn`
- `refactor(extract): kotlin module index has one declaring-file rule; module_target takes refs`
- `refactor(extract): ts receiver walk seeds inferred bindings instead of a locals stack; type params stack`

Trailer on each: `Refs-Issue: @flash-review-cleanups`. Do not push.

## Deliverable

`REPORT.md` at the worktree root. Overwrite the stale one there. Tables only. Sections:

1. `## Commits` : sha, subject, files.
2. `## Rows` : row number, fixed or closed, receipt, one line.
3. `## Verify` : the row-6 stderr diff result, RATCHET.tsv diff result.
4. `## Tests changed` : test, old expectation, new expectation, why.
5. `## Gate` : last 3 lines.
6. `## Blocked` : empty, or the exact error and the diff you wanted.

## Laws

- Stop and write `## Blocked` when a command fails in a way this brief did not anticipate.
- No em dashes. No praise. Facts and receipts.
- Rust comments: at most 2 consecutive comment lines; state only constraints the code cannot show.
- Tests are integration tests through the real `extract` binary on fixture files. No mocks, no fakes. Expected values are hand-derived from fixture bytes, never copied from the binary's output.
- Behavior-preserving: a resolved edge, an origin, or a drop reason that changes is a bug in your change, never a new expectation.
