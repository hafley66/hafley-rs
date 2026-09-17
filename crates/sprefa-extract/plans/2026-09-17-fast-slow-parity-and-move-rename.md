# Plan: fast/slow parity and move/rename without a compiler

Date: 2026-09-17. Companion to `docs/0_architecture-matrix-20260917.md`, which carries the matrix, boards, and vocabulary this plan cites.

## TOC

1. Lanes 0-5
2. Verification

## 1. Lanes

### Lane 0: write this down in the repo (first action after approval)

| step | change |
| --- | --- |
| 0.1 | sections 2-7 of this file become `crates/sprefa-extract/docs/0_architecture-matrix-20260917.md` (TOC first, same tables and boards), committed on `worktree-extract-fixes` |
| 0.2 | section 8-9 become `crates/sprefa-extract/plans/2026-09-17-fast-slow-parity-and-move-rename.md` |
| 0.3 | `AGENTS.md:52-54` corrected: `ArgToParam` / `RetToCallRes` are emitted; `types.rs:3665-3669` stale comment on `resolved_import` corrected |

Order is by dependency. Each lane is a commit set on `worktree-extract-fixes`, gated by `cd crates/sprefa-extract && cargo test --features cli --no-fail-fast` (173 binaries; the 2 pre-existing `golden_parity` oracle-prefix failures stay until their own lane).

### Lane 1: the accuracy meter (issue AC 3, 4, 5)

Joins fast to slow on span so every later lane is measured.

| step | change | files |
| --- | --- | --- |
| 1.1 | `scip_def` / `scip_ref` / `scip_fn_edge` rows gain `start`,`end` copied from the defining/referring occurrence | `src/scip_rows.rs:116-381`, `src/types.rs:3400-3480` row structs, `schema/1_facts.tsp` + regen |
| 1.2 | `extract diff --fast a.db --slow b.db --relation call` : per-language table `true / false_positive / miss` by `resolution_origin`, joining `resolved_edge.caller_site_start..end` to `scip_occurrence.start..end` with role READ | new `src/5_diff.rs`, bin arm in `src/bin/extract.rs` next to `fast`/`slow` |
| 1.3 | test: `tests/130_fast_slow_diff.rs` over `tests/fixtures/{rust,ts,go}` using the real indexers (same law as `golden_parity.rs`: never fake green), asserts the table shape and pins today's numbers as a ratchet | new |

### Lane 2: module plane first, everywhere

| step | change | files |
| --- | --- | --- |
| 2.1 | thread `KtModuleIndex` into kotlin's `resolve_call` / `resolve_type_dst`; stamp `ModulePlane` | `src/lang/kotlin.rs:1728-1830`, `src/project.rs:429` |
| 2.2 | same for `PyModuleIndex` | `src/lang/python/_0_source.rs:2628-3381`, `src/project.rs:420` |
| 2.3 | leg order per language: module_plane, spelled receiver, self_type, param-typed, receiver-typed, then guess legs; the spelled-receiver leg is new for rust (`CstProjector.project`, `Type::f`) | `src/lang/rust.rs:1154-1222`, `ts.rs:4938-4953`, `go.rs:4232-4301` |
| 2.4 | local bindings shadow the corpus: a param/let/closure-param named X blocks `corpus_unique` and `same_file` for calls to X in that scope (python F1 `tests/90_mutation_battery.rs:389`, rust `push` closure param in `wire.rs`) | per-language resolve, using the df `Param`/`LetBind` nodes already emitted |
| 2.5 | guess legs demoted: a method call with an untyped receiver lands in `unresolved reason=ambiguous`; `corpus_unique` only answers free calls | `src/lang/rust.rs:459,493,1222` and the ts/go/python/kotlin twins |
| 2.6 | ratchet in `tests/130_fast_slow_diff.rs` moves: false positives must drop to 0 on the fixtures, misses may rise and are listed by reason | |

### Lane 3: one vocabulary, two providers (issue AC 1)

| step | change | files |
| --- | --- | --- |
| 3.1 | `trait FactProvider { fn defs, fn refs, fn call_edges, fn imports }` returning the same row types; `FastProvider` reads `node`/`resolved_edge`/`resolved_import`, `ScipProvider` reads `scip_occurrence`/`scip_relationship` | new `src/6_provider.rs` |
| 3.2 | `extract fast` and `extract slow` both emit the provider-neutral tables (`def`, `ref`, `call_edge`, `import_edge`) with a `provider` column, alongside their native rows | `src/bin/extract.rs`, `schema/1_facts.tsp` |
| 3.3 | lane 1's diff reads the neutral tables, so any future plane joins for free | `src/5_diff.rs` |

### Lane 4: move and rename for go and python

| step | change | files |
| --- | --- | --- |
| 4.1 | `impl Rehome for GoSource`: `import_refs` from `GoModuleIndex`, `respell` rewrites the import path segment; `go.mod` module line via `RehomeManifests` | new `src/lang/go_rehome.rs`, roster `src/lang/mod.rs:140` |
| 4.2 | `impl Rename for GoSource`: package-scope identifiers via the module index + tree-sitter scopes; exported names across importing packages; method rename only with a spelled receiver, else `RenameStop` | new `src/lang/go_rename.rs`, roster `:184` |
| 4.3 | python twins on `PyModuleIndex`; `from x import y` and `import x as z` respells; `__init__.py` re-exports followed | new `src/lang/python/_3_rehome.rs`, `_4_rename.rs` |
| 4.4 | tests mirror `3_move_rust.rs` / `5_rename_rust.rs` shapes: `tests/6_move_go.rs`, `9_rename_go.rs`, `6_move_python.rs`, `9_rename_python.rs`, plus `--verify-scip` cross-check on the fixtures with real `scip-go` / `scip-python` | new |

### Lane 5: named stops, not in this plan

| item | why out |
| --- | --- |
| bash, dockerfile, k8s yaml, gdscript beyond cst | new front-ends; the `sprefa-extract-add-language` skill owns that recipe; k8s yaml already reaches the `data` plane |
| `golden_parity` oracle prefix (2 failing tests) | own lane, path-prefix rewrite in 11 oracle files |
| `LambdaElem` / `LambdaRet` flow edges | program-level work per AGENTS.md |
| build-vs-buy for python module resolution | candidates to evaluate before lane 4.3: `ruff_python_resolver` (Pyright's algorithm, Astral), `ruff_python_semantic`; rust module plane alternative `ra_ap_hir_def` DefMap without type inference. Table with sizes and license lands at the top of lane 4 before any code |

## 2. Verification

| lane | command | pass condition |
| --- | --- | --- |
| all | `cd crates/sprefa-extract && cargo test --features cli --no-fail-fast` | only the 2 known `golden_parity` failures |
| 1 | `extract fast --sqlite f.db <files>; extract slow --sqlite s.db .; extract diff --fast f.db --slow s.db` on `crates/sprefa-extract/src` | table prints per origin; `push` shows 148 false before lane 2 |
| 2 | same diff | false positives 0 on fixtures; `CstProjector.project` callers 4 of 4 |
| 3 | `sqlite3 f.db 'select provider, count(*) from call_edge group by 1'` on both dbs | both providers fill the same table |
| 4 | `extract move`, `extract rename --verify-scip` on the go/python fixtures | plan equals the scip second opinion; verify command green |

