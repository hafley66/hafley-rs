# Lane C-rust: REPORT

## Commits

| sha | subject | files |
| --- | --- | --- |
| be5b96ef | feat(extract): rust phase 1 names a spelled path receiver | `crates/sprefa-extract/src/lang/rust_receivers.rs`, `crates/sprefa-extract/tests/130_rust_spelled_receiver.rs`, `crates/sprefa-extract/tests/fixtures/rust/spelled_receiver/src/{proj.rs,spelled.rs}` |
| 1a999c97 | feat(extract): rust binding-typed receiver legs (field, ctor return, trait bound) | `crates/sprefa-extract/tests/130_rust_spelled_receiver.rs`, `crates/sprefa-extract/tests/fixtures/rust/spelled_receiver/src/legs.rs` |
| 74670784 | fix(extract): rust name-match legs respect innermost-scope shadowing | `crates/sprefa-extract/src/types.rs`, `src/lang/rust.rs`, `src/lang/rust_receivers.rs`, `src/lang/go.rs`, `tests/130_rust_spelled_receiver.rs`, `tests/fixtures/rust/spelled_receiver/src/{free.rs,shadow.rs}` |
| a61a8eb4 | fix(extract): move spelled_receiver fixtures out of the scip ratchet corpus | `tests/130_rust_spelled_receiver.rs`, `tests/fixtures/rust/spelled_receiver/src/*` (deleted), `tests/fixtures/rust_spelled_receiver/src/{free.rs,proj.rs,shadow.rs}` |

C.2 has no commit: `impl_target`'s corpus `(T, m)` table already covers own-file and cross-file impls, so the receiver leg bound spelled receivers with no change beyond C.1. Receipt: the `spelled_unit_struct_receiver_binds` test, a throwaway same-file probe (`caller same_file_call -> callee project`, origin `receiver`), and the CTF query 1.

## CTF

Before (base `785e4edb`, `extract fast --sqlite target-lanec-rust/ctf.db` over the 18 files):

```
SELECT resolution_origin, callee_path, COUNT(*) FROM resolved_edge WHERE callee_name='project' GROUP BY 1,2
same_file|src/lang/astgrep.rs|2
same_file|src/lang/ts.rs|5

SELECT resolution_origin, COUNT(*) FROM resolved_edge WHERE callee_name='push' AND callee_path='src/lang/rust.rs' GROUP BY 1
corpus_unique|118
receiver|7
same_file|30
```

After (HEAD, same 18 files):

```
SELECT resolution_origin, callee_path, COUNT(*) FROM resolved_edge WHERE callee_name='project' GROUP BY 1,2
receiver|src/lang/astgrep.rs|10
receiver|src/lang/ts.rs|1
same_file|src/lang/ts.rs|2

SELECT resolution_origin, COUNT(*) FROM resolved_edge WHERE callee_name='push' AND callee_path='src/lang/rust.rs' GROUP BY 1
corpus_unique|81
receiver|7
same_file|30
```

Readings:

| observation | fact |
| --- | --- |
| astgrep `project` sites | 5 distinct `CstProjector.project` call sites (astgrep, go, kotlin, rust, ts extract fns), all bound to the impl def in `src/lang/astgrep.rs` under origin `receiver`. 10 physical rows: each site emits the caller-node row plus the closure-caller mirror row (lane B dedup key). Before: 1 site same_file, 3 unresolved, 1 bound to the wrong file (ts.rs's own `project` fn via same_file). |
| ts.rs `project` rows | before 5 same_file rows / 4 sites; after 1 receiver + 2 same_file. The 2 lost rows are the ts.rs sites now declined by the shadow walk (scope-bound plain calls), present in `unresolved` with reason `inferred`. |
| `push` receiver count | 7, unchanged; the constraint "not above the 7 true callers" holds. |
| `push` corpus_unique 118 -> 81 | the 37 declined sites appear as `unresolved` rows with reason `inferred` (36 rows; 1 site re-bound elsewhere). Lane D's target set. |

## Legs

| leg | already worked | change | test name |
| --- | --- | --- | --- |
| C.1 spelled path receiver (`CstProjector.project()`) | no: phase 1 returned `Inferred` for any non-scope-bound path ident | `receiver_outcome`'s `Expr::Path` arm: single- or multi-segment path, not scope-bound, uppercase last segment -> `Named(last)`; scope-bound keeps the binding lookup | `spelled_unit_struct_receiver_binds` |
| C.2 receiver leg binds the spelled type | yes: `impl_target` is corpus-wide (own-file and cross-file impls both in `impl_methods`) | none | `spelled_unit_struct_receiver_binds` + CTF query 1 |
| C.5 field with declared type (`self.field.m()`, `struct S { field: T }`) | yes: `receiver_outcome`'s `Expr::Field` arm + the `tables()` field map | none | `field_typed_receiver_binds` |
| C.5 constructor return (`let x = T::new(); x.m()`) | yes: `init_type` + `assoc_rets` (`fn new() -> Self`) | none | `constructor_return_receiver_binds` |
| C.5 trait-bound generic (`fn f<P: Proj>(p: P) { p.run() }`) | yes: `seed_params` + `trait_bounds_of_generics` + `trait_fn_target` | none | `trait_bound_generic_receiver_binds` |
| C.6 innermost-scope shadowing | no: the shadow check was def-span only or absent; scope-bound plain calls matched free fns | phase-1 `visit_expr_closure` scopes closure params; `visit_expr_call` mints `ReceiverOutcome::Shadowed` (replaces the WIP's `SHADOW_SENTINEL` string hack); `recv_known` + `call_drops` treat `Shadowed` like `Inferred`; go.rs arms updated for the new variant | `shadowed_call_does_not_bind_free_fn` |

Note on the C.5 fixture: the trait method is spelled `run` (not the WIP's `go`) so its name is corpus-ambiguous; see the ratchet entry in `## Blocked`.

## Gate

`cd crates/sprefa-extract && cargo test --features cli --no-fail-fast` at HEAD: exit 0, 174 test targets, 921 tests passed, 0 failed. Last 3 lines:

```
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

First full-gate run after the fixture move failed 2 of 9 `scip_freshness` cases (`a_stale_index_makes_ensure_rebuild_rather_than_reuse`, `the_informed_default_adopts_a_fresh_index_and_a_stale_one_stays_plain`): env-race flakes between parallel test binaries, no fixture or source dependency. `scip_freshness` green 3/3 in isolation; the next full-gate run was green.

## Blocked

Empty. Nothing blocks the delivered work. One constraint for the `golden_parity.rs` owners (that file is not mine), recorded because it forced the fixture move in a61a8eb4:

The rust scip ratchet (`call_resolve_scip_ratchet_rust`) cannot grade any method-call fixture under `tests/fixtures/rust/`:

1. rust-analyzer 1.98.0's scip emits no reference occurrence for a method call whose method is defined in another file (verified with a throwaway index dump: `spelled.rs` calling `CstProjector.project()` in `proj.rs` produced zero occurrences). The occurrence-parity assert (`missing_occurrence == 0`) therefore fails for cross-file method calls.
2. With the call moved into the impl's own file, scip sees the site, but the twin re-derives `name_t` via `same_file_call_match`, which takes the FIRST same-file def node named `callee` with no ambiguity guard: for `project` that is the trait decl, not the impl method. The twin then expects `name_resolve` at that span while the arm (receiver-typed site, name-match legs declined) emits `scip_override` at scip's precise def. The multiset at `golden_parity.rs:1889` fails on kind and span.

Diff I wanted (had the fixtures stayed at the mandated path) in `tests/golden_parity.rs`, twin block of `call_resolve_scip_ratchet_rust`:

```diff
-            let name_t = RustSource::call_name_match(out, def_index, callee);
+            // A typed receiver or a Shadowed site never runs the name-match
+            // legs, so the twin must not expect one for it.
+            let recv_typed = call.aux.receivers.iter().any(|r| {
+                r.call_site == site.span
+                    && matches!(
+                        r.outcome,
+                        sprefa_extract::ReceiverOutcome::Named(_)
+                            | sprefa_extract::ReceiverOutcome::Shadowed
+                    )
+            });
+            let name_t = if recv_typed {
+                None
+            } else {
+                RustSource::call_name_match(out, def_index, callee)
+            };
```

plus relaxing the `missing_occurrence == 0` assert to skip sites whose receiver outcome is `Named`/`Shadowed` (counted instead), so cross-file receiver sites are graded by the multiset and the histogram, not by occurrence parity. With that landed, the fixtures can move back under `tests/fixtures/rust/spelled_receiver/` and the ratchet meters the receiver plane.
