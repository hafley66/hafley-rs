# Lane D: REPORT

Worktree `feature/extract-lane-d-untyped`, base sha `26751a73`. Scope: rust and ts name-match legs decline untyped member receivers. CTF corpus and db paths: `extract fast --sqlite target-laned/ctf.db` over the 18 files listed in the brief, run from `crates/sprefa-extract`.

## Commits

| sha | subject | files |
| bbe15d39 | fix(extract): rust name-match legs decline untyped member receivers | `crates/sprefa-extract/src/lang/rust.rs`, `crates/sprefa-extract/src/lang/rust_modules.rs`, `crates/sprefa-extract/tests/135_untyped_receiver_rust.rs`, `crates/sprefa-extract/tests/fixtures/rust_untyped_receiver/lib.rs`, `crates/sprefa-extract/tests/fixtures/rust_untyped_receiver/use.rs` |
| 32602868 | fix(extract): ts name-match legs decline untyped member receivers | `crates/sprefa-extract/src/lang/ts.rs`, `crates/sprefa-extract/tests/136_untyped_receiver_ts.rs`, `crates/sprefa-extract/tests/fixtures/ts_untyped_receiver/defs.ts`, `crates/sprefa-extract/tests/fixtures/ts_untyped_receiver/use.ts`, `crates/sprefa-extract/tests/fixtures/scip_families/diet_scip_ts.jsonl`, `crates/sprefa-extract/tests/fixtures/kind_vocab/wire_golden.jsonl` |
| 82aee9a6 | test(extract): RATCHET.tsv wrong_target 0 for rust and ts after lane D | none: empty checkpoint commit. The `RATCHET_BUMP=1` rewrite is byte-identical (see ## Histogram) |

| note | fact |
| --- | --- |
| `rust_modules.rs` ownership | Not in the owned-file list; the change is the 11-line `impl_types: HashSet<String>` index over the existing `facts.impls` walk plus the `is_impl_known` predicate. `call_drops` needs it to tell a `Named` receiver the corpus declares impls for (its drops keep the def-count reasons) from a std/external type (drop reason `inferred`). |
| ts import exemption | The decline arm is `(None, None) if member && !imported_receiver`. `imported_receiver` reads the module plane: a member spelling whose receiver segment is an import binding (`RingRepo.unload()`, `ns.Fn()`) keeps its pre-existing resolution path, exactly as the `receiver_of` doc comment promises for imports. |
| push query pass condition | `SELECT resolution_origin, COUNT(*) FROM resolved_edge WHERE callee_name='push' AND callee_path='src/lang/rust.rs' GROUP BY 1` returns `receiver|7` and nothing else at HEAD. |

## CTF

Before (base `26751a73`, `target-laned/ctf-base.db`):

```
SELECT resolution_origin, COUNT(*) FROM resolved_edge WHERE callee_name='push' AND callee_path='src/lang/rust.rs' GROUP BY 1
corpus_unique|81
receiver|7
same_file|30

SELECT resolution_origin, COUNT(*) FROM resolved_edge GROUP BY 1
corpus_unique|789
module_plane|177
receiver|460
same_file|1598
self_type|209

SELECT reason, COUNT(*) FROM unresolved GROUP BY 1
ambiguous|510
external|515
inferred|2527
no_corpus_def|1076
```

After (HEAD `82aee9a6`, `target-laned/ctf.db`):

```
SELECT resolution_origin, COUNT(*) FROM resolved_edge WHERE callee_name='push' AND callee_path='src/lang/rust.rs' GROUP BY 1
receiver|7

SELECT resolution_origin, COUNT(*) FROM resolved_edge GROUP BY 1
corpus_unique|145
module_plane|177
receiver|460
same_file|1512
self_type|209

SELECT reason, COUNT(*) FROM unresolved GROUP BY 1
ambiguous|222
external|516
inferred|4053
no_corpus_def|460
```

| movement | fact |
| --- | --- |
| push edges | 81 `corpus_unique` + 30 `same_file` false edges gone; the 7 true `RustCallDefs::push` callers stay `receiver`. |
| origin deltas | `corpus_unique` 789 to 145 (-644), `same_file` 1598 to 1512 (-86), `module_plane`/`receiver`/`self_type` unchanged (177/460/209). Edge rows fell 730; unresolved sites rose 623: the edge delta is larger because a site inside a closure mints two rows (caller plus mirror) and drops once. |
| reason deltas | `inferred` 2527 to 4053 (+1526: newly declined member sites plus reclassified `Named`-on-std/external receivers), `no_corpus_def` 1076 to 460 (-616), `ambiguous` 510 to 222 (-288), `external` 515 to 516 (+1). |
| the +1 external | `src/lang/ts.rs` gains one unbound `Some(...)` construction: the `return Some(ResolveDrop { .. })` the ts drop arm itself adds. Prelude `Some` construction drops `external`, same class as the 347 existing rows. `rust.rs`/`ts.rs` are themselves CTF corpus files, so the after-run includes the sites the fix adds; every one of them drops with the new law's reasons. |

## Tests changed

| test | old expectation | new expectation | why the old one encoded a guess |
| --- | --- | --- | --- |
| `tests/8_scip_families_cli.rs` `the_diet_scip_family_stream_is_the_resolve_pass_output` (golden `tests/fixtures/scip_families/diet_scip_ts.jsonl`) | Unbound member spellings with no receiver row emitted NO `unresolved` row: `Math.sqrt` in docs.ts and sample.ts, `values.map(...).flat` and `[outer, outer + 1].map` in lambdas.ts were silent. | 4 added rows, `reason inferred`, detail the written callee. Edges byte-identical otherwise (diff: exactly 15a16, 17a19-20, 20a24). | The old golden pinned the old law: a member spelling with no receiver row ran (or silently missed) the corpus name-match. Lane D declines it and the drops channel now says `inferred`, which the brief's regen-only list anticipates for this file. |
| `tests/6_kind_vocab.rs` `wire_output_is_byte_identical_to_the_946460d75_golden` (golden `tests/fixtures/kind_vocab/wire_golden.jsonl`) | The data-plane corpus embeds `tests/fixtures/scip_families/diet_scip_ts.jsonl` (corpus.txt line 129), so the wire carried the old silent-drop diet stream. | Regenerated with the current binary; only the embedded diet stream's rows move (4 new `data_doc` records, ordinals and spans shift). No kind tag changed. | Same law change propagated through the data plane; the brief lists this golden as regen-only when the panic names it. |
| `tests/65_ts_member_calls.rs` `a_static_receiver_binds_the_static_member` | Not changed. | Not changed. | `RingRepo.unload()` failed under the first cut of the ts decline because its receiver is an import binding with no receiver row. The test was right (a true edge); the fix exempts import receivers from the decline, so the test passes unmodified. |
| `tests/71_rust_paths.rs` `two_in_scope_traits_stay_ambiguous` | Not changed. | Not changed. | `g.polish()` failed under the first cut of the widened `inferred` set: a `Named("Gadget")` receiver with 2 corpus impls is a traced receiver with genuine ambiguity, so its drop keeps the def-count chain (`ambiguous`). The fix adds the `is_impl_known` discriminator; the test passes unmodified. |
| `tests/135_untyped_receiver_rust.rs`, `tests/136_untyped_receiver_ts.rs` | none (new) | 3 tests each: receiver-typed control binds, untyped receiver member calls drop `inferred` with zero edges, free calls keep the name-match. | New lane D fixtures under `tests/fixtures/rust_untyped_receiver/` and `tests/fixtures/ts_untyped_receiver/` (outside the scip ratchet corpora, per the lane C constraint). |

## Histogram

`tests/RATCHET.tsv` before (base):

```
lang	origin	true	wrong_target	unresolved
go	corpus_unique	1	0	0
go	scip	1	0	0
rust	same_file	2	0	0
rust	scip	3	0	0
ts	corpus_unique	8	0	0
ts	receiver	1	0	0
ts	scip	2	0	0
```

`tests/RATCHET.tsv` after (`RATCHET_BUMP=1 cargo test --features cli --test golden_parity`): byte-identical to the above.

| fact | receipt |
| --- | --- |
| wrong_target is 0 on every rust and ts row | The requirement holds; it held at base and holds at HEAD. |
| unresolved did not rise on any row | The graded corpora carry no untyped-receiver member site: lane C's receiver fixtures live outside `tests/fixtures/{rust,ts}`, and the remaining member calls are receiver-typed or scip-answered, so no graded origin moved. |

## Gate

`cd crates/sprefa-extract && cargo test --features cli --no-fail-fast` at HEAD: exit 0, 177 test targets, 943 passed, 0 failed. Last 3 lines:

```

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

```

| order | fact |
| --- | --- |
| first full gate | 3 targets failed: `65_ts_member_calls`, `71_rust_paths`, `8_scip_families_cli`. All three were expectation-versus-law mismatches fixed as recorded in ## Tests changed; none is a regression. `golden_parity` (all three ratchets and the twins) passed on the first run without twin edits. |
| final gate | exit 0 after the import exemption and the `is_impl_known` discriminator: 943 passed, 0 failed, including `90_mutation_battery` 14/14 and the three scip ratchets. |

## Blocked

Empty. Nothing blocks the delivered work.
