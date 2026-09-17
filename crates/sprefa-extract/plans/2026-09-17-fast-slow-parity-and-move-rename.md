# Plan v2: fast/slow parity and move/rename without a compiler

Date: 2026-09-17. Companion to `docs/0_architecture-matrix-20260917.md`. v1 of this plan was reviewed by three adversarial lanes (`plans/reviews/2026-09-17-plan-review-{1-premise,2-design,3-moverename}.md`); v2 is what survived.

## TOC

1. What the reviews changed
2. Lanes, in dependency order
3. Verification
4. Named stops

## 1. What the reviews changed

| v1 step | verdict | replacement |
| --- | --- | --- |
| 1.1 spans on `scip_def`/`scip_ref`/`scip_fn_edge` | dropped: `scip_occurrence` already has spans; the projections are per (file, symbol) and would multiply | join to `scip_occurrence` directly |
| 1.2 `extract diff` CLI verb, `role READ` join, span equality | dropped: indexers set no role bits (`src/1_rename_verify.rs:246-258`); site spans differ from occurrence spans in 3 of 5 languages; `resolved_edge` has no target span; one site yields N rows (1,966 duplicate groups); a metric is a program per `AGENTS.md` | lane B: the join lives in `tests/`, reuses `site_occurrence` + `definition_of` (`src/scip.rs:917-959`), keys on (path, site span, target), and `resolved_edge` gains the target span |
| 1.3 new ratchet test | dropped: `golden_parity.rs:946,1257,1555` already run real indexers and assert missing/disagree/overbound = 0 | lane B adds an origin-keyed counter to those three |
| 2.3 module plane first | amended: it would regress value bindings (`use crate::util;` + `fn run(util: &CstProjector) { util.project() }`, `rust.rs:1151-1157` is right today) | lane C: receiver leg stays first; the fix is phase-1 capture of spelled receivers so the receiver leg can fire |
| 2.4 shadowing keyed by name | amended: over-blocks the whole enclosing def; must gate every name-match leg including the module plane | lane C: innermost enclosing scope span, gates all name legs |
| 2.5 demote `corpus_unique` | amended: the 155 `push` edges are `corpus_unique` 118 + `same_file` 30 + `receiver` 7; the rule is per receiver shape, and the reason vocabulary was wrong | lane D: an untyped receiver loses every name-match leg; reason `inferred` for unknown receiver, `no_corpus_def` when no candidate, `ambiguous` only for 2+ candidates |
| 3.1 `trait FactProvider` | dropped: a table rename; the checker tier already writes `resolved_edge` under origin `Checker` and is site-shaped (`call_at(path, span, callee)`) | one neutral relation per call occurrence, as a view or in-test join, no new tables |
| 4 go/python arms | kept, expanded: 15 rust-arm capabilities were unlisted; python top-level rename is 2/7 deterministic without attribute-access and `__all__` seats; go interface satisfaction has no phase-1 row so `RenameStop` is unreachable | lane E carries the capability table and the new phase-1 rows it needs |
| 5 `ruff_python_resolver` candidate | dropped: not on crates.io; `ruff_python_semantic` 0.0.14 does scopes, not import-to-file; `ra_ap_hir_def` 0.0.352 collides with the 0.0.349 pin | python module plane stays bespoke with its holes listed |

Reviewed leg order (premise review, "Actual leg order"): rust `receiver -> module_plane (qualified) -> self_type -> same_file -> module_plane (import) -> corpus_unique -> scip fold -> checker`; go `receiver -> shadowing receiver -> module_plane (dir) -> alias_chain -> same_file -> corpus_unique -> module_plane (dot import)`; ts `module_plane -> receiver -> corpus_unique`.

## 2. Lanes, in dependency order

Each lane is a commit set on `worktree-extract-fixes`, gated by `cd crates/sprefa-extract && cargo test --features cli --no-fail-fast` (170 integration targets; the 2 pre-existing `golden_parity` oracle-prefix failures stay until lane F).

### Lane A: stale text (docs only)

| step | change | files |
| --- | --- | --- |
| A.1 | `AGENTS.md:50-55`: `ArgToParam`/`RetToCallRes` are emitted by `flow_edges`; `LambdaElem`/`LambdaRet` reserved | `AGENTS.md` |
| A.2 | `types.rs:3664-3667`: all five languages emit `resolved_import` | `src/types.rs` |
| A.3 | `tests/90_mutation_battery.rs:26-29`: rust does answer `corpus_unique` | `tests/90_mutation_battery.rs` |

### Lane B: the meter (one relation, in tests)

| step | change | files |
| --- | --- | --- |
| B.1 | `ResolvedEdge` gains `callee_start`, `callee_end` copied from `ProjectEdge` (`types.rs:3300-3305` already carries them); sqlite schema regen | `src/types.rs:3320-3334`, `src/wire.rs`, `schema/1_facts.tsp` + `schema/2_gen.mjs` |
| B.2 | edge key = (path, site span, target blob, target span); flatten dedups per key so a site inside a closure or spliced macro counts once | `src/wire.rs` `flatten_resolved`, cf. `golden_parity.rs:1046-1052` |
| B.3 | `RatchetCounts` (`golden_parity.rs:1828-1837`) gains a per-`resolution_origin` histogram of true / wrong-target / unresolved, printed and pinned in the existing `RATCHET.tsv` with the `RATCHET_BUMP` discipline (`tests/ratchet_recall.rs:1-12`); externals counted as today (`golden_parity.rs:1160-1162`) | `tests/golden_parity.rs`, `tests/RATCHET.tsv` |
| B.4 | the three ratchets bind both sides on `_content_id` and refuse a run whose per-language join coverage is zero (no silent 0/0/0) | `tests/golden_parity.rs` |

### Lane C: make the deterministic legs able to fire

| step | change | files |
| --- | --- | --- |
| C.1 | phase 1 records the spelled receiver for path-expression receivers: `CstProjector.project(..)` (unit struct), `Type::f(..)`, `module::f(..)`; today `site.callee_path` is NULL for these (premise review, CTF query 3) | `src/lang/rust.rs` call projector (`:2079` qualified-call site), `CallSite` in `src/types.rs` |
| C.2 | receiver leg accepts a spelled receiver whose name is a corpus type with the method in its impl table (`recv_t` at `rust.rs:1077-1101`) | `src/lang/rust.rs:1151-1158` |
| C.3 | thread `KtModuleIndex` into kotlin's call/type resolve; stamp `ModulePlane`; add a package-scope index so same-package bare names bind (`kotlin_modules.rs:8-11` today binds nothing) | `src/lang/kotlin.rs:1728-1830`, `src/lang/kotlin_modules.rs`, `src/project.rs:429` |
| C.4 | thread `PyModuleIndex` into python's call/type resolve; star imports honor `__all__` and skip `_` names; add a same-file leg (python mints none today) and an enclosing-class leg for `self.m()` | `src/lang/python/_0_source.rs:2628-3381`, `_2_modules.rs`, `src/project.rs:420` |
| C.5 | binding-typed legs that the data already supports: field with declared type (`df_field` + `sig`), constructor-return (`sig{slot=ret}` for `x := NewFoo(); x.Bar()`), trait-bound generic (`Generic`/`Param` edges + `IfaceImpl`) | `src/lang/rust.rs`, `src/lang/go.rs:4235-4245`, `src/lang/ts.rs:4945-4959` |
| C.6 | shadowing keyed by scope: a `Param`/`LetBind`/closure-param named X blocks every name-match leg (module plane included) for calls to X inside the innermost enclosing `Closure` node span, else the def span (python F1 `tests/90_mutation_battery.rs:389`, `python/_0_source.rs:3410-3416` is the def-only version) | per-language resolve |

### Lane D: name-match legs stop answering untyped receivers

| step | change | files |
| --- | --- | --- |
| D.1 | rule per receiver shape: a member call whose receiver no leg typed gets no `same_file` and no `corpus_unique` answer; free calls keep both | `src/lang/rust.rs:1193-1225`, `ts.rs:4945-4955`, `go.rs:4356-4381`, python/kotlin twins |
| D.2 | reasons: unknown receiver type -> `inferred`; no corpus candidate -> `no_corpus_def`; 2+ candidates -> `ambiguous` | `src/lang/rust.rs:1414-1426` and twins |
| D.3 | lane B histogram moves: wrong-target count per origin must reach 0 on the fixtures; unresolved rises and is listed by reason; the design review's misses table (`plan-review-2-design.md` section 2) is the checklist: each row either recovers through a lane C leg or lands in `inferred` | `tests/RATCHET.tsv` |

### Lane E: move and rename for go and python

Prerequisites: C.3, C.4, D.1. The rust arm is the template; every row below is a capability the rust arm has (`plan-review-3-moverename.md` section "Lane 4 gaps") and the go/python arm must either implement or name as a stop.

| capability | rust receipt | go | python |
| --- | --- | --- | --- |
| directory-standing file | `rust_rehome.rs:82` `directory_stem = "mod"` | none (stop) | `__init__.py` |
| import refs through the language's file law | `:86-185` | import path + `internal/` check + package clause vs destination dir | clauses + relative-dot recompute on depth change |
| moved file parsed unconditionally, stayers prefiltered | `:155-176`, `carries_name :1577` | same | same |
| batch-aware respell, one ref per span | `:186-209` | import path segment + package clause | `import x as z`, `from a.b import n`, `__init__` re-exports |
| plan check before staging | `:211` `RehomePlanCheck` | no parent package -> stop | layout vs package disagreement -> stop |
| include-style literals | `:38` `INCLUDE_MACROS`, `:807` | `//go:embed` patterns (report, no rewrite) | `importlib.resources`, `importlib.import_module` strings (report, no rewrite) |
| manifests | `:217`, `:1413` target tables | `go.mod` module line, `go.work` `use` | `pyproject.toml` `[project.scripts]`, `packages` list |
| test twin | n/a | `_test.go` siblings move with the package | `test_*.py` untouched, `conftest.py` reported |
| receipts on every text edit | `:815` `Respell` | same vocabulary | same vocabulary |
| rename seats | `rust_rename.rs` | package-scope idents; exported names across importers; exportedness change (`Old` -> `old`) is a stop; method rename only with a spelled receiver else `RenameStop`; interface satisfaction needs a new phase-1 row: `method_decl{type, name}` + `iface_method{iface, name}` so the stop is knowable | module-level defs via import clauses; `m.old` attribute seats after `import m`; `__all__` strings reported; `getattr`/`importlib` strings reported; method rename with spelled receiver only |
| tests | `3_move_rust.rs` (18), `5_rename_rust.rs` (8) | `6_move_go.rs`, `9_rename_go.rs` mirroring those shapes + `--verify-scip` with real `scip-go` | `6_move_python.rs`, `9_rename_python.rs` + `--verify-scip` with real `scip-python` |

### Lane F: oracle prefix

| step | change | files |
| --- | --- | --- |
| F.1 | rewrite `v6/sprefa-extract/` to `crates/sprefa-extract/` in the 11 captured oracles, or regenerate via `cargo run --example v5_normalize` per the panic text | `tests/fixtures/**/*.v5.jsonl` |

## 3. Verification

| lane | command | pass condition |
| --- | --- | --- |
| all | `cd crates/sprefa-extract && cargo test --features cli --no-fail-fast` | only the 2 known `golden_parity` failures until lane F, then 0 |
| B | `cargo test --features cli --test golden_parity call_resolve_scip_ratchet_` | histogram printed per origin; nonzero coverage per language; `RATCHET.tsv` pinned |
| C | `extract fast --sqlite f.db <18 CTF files>` then `SELECT resolution_origin, COUNT(*) FROM resolved_edge WHERE callee_name='project'` | 5 `CstProjector.project` sites bound to `src/lang/astgrep.rs`, origin `receiver` |
| D | same db, `SELECT resolution_origin, COUNT(*) FROM resolved_edge WHERE callee_name='push' AND callee_path='src/lang/rust.rs'` | rows = the 7 true `RustCallDefs::push` callers; the other 148 sites in `unresolved` with reason `inferred` |
| E | `extract move`, `extract rename --verify-scip` on the go/python fixtures | plan equals the scip second opinion; verify command green |

## 4. Named stops

| item | why out |
| --- | --- |
| bash, dockerfile, k8s yaml, gdscript beyond cst | new front-ends via the `sprefa-extract-add-language` skill; k8s yaml already reaches the `data` plane |
| `LambdaElem` / `LambdaRet` flow edges | program-level work per `AGENTS.md` |
| method/field rename without a spelled receiver | 0 deterministic in every language (review 3 verdict table); the tool refuses, the slow lane (`--verify-scip`, checker tier) answers |
| a bought python import resolver | none published for Rust; bespoke plane stays, holes listed in C.4 |
