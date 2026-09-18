# Lane C-gots, second run: ts C.6 scope shadowing

Scope cut by the user: ts only. Go and rust C.6 are in the base. This report lives under
`plans/reviews/` per 6858d828 and the a1bfc114 precedent; the worktree-root `REPORT.md` is a
tracked boop-feature report (0785495b) this lane does not own and did not touch.

## Commits

| sha | subject | files |
| --- | --- | --- |
| 763366cf | fix(extract): ts name-match legs respect innermost-scope shadowing | `src/lang/ts_receivers.rs`, `src/lang/ts.rs`, `tests/134_ts_binding_legs.rs`, `tests/fixtures/ts_binding_legs/{free.ts,shadow.ts}` |
| 49c14b66 | feat(extract): go binding-typed receiver legs (field, ctor return, interface param) | prior lane, in base, skipped |
| eb9c58a6 | feat(extract): ts binding-typed receiver legs (field, ctor return, generic bound) | prior lane, in base, skipped |
| 74670784 | fix(extract): rust name-match legs respect innermost-scope shadowing (rust C.6 + go decline arms + `ReceiverOutcome::Shadowed` in `src/types.rs`) | prior lane, in base, skipped |

C.5 has no commit this run: both languages already committed by 49c14b66/eb9c58a6 and merged by
21226a6b. Receipts re-run green (134: 7 tests, including the four C.5 tests).

## Cells

| lang | leg | before (origin or none) | after | test name |
| --- | --- | --- | --- | --- |
| ts | param shadow: `function run(project: () => void) { project() }`, free `project` in file 1 | 1 `resolved_edge` run -> project, origin `corpus_unique` | no edge; `unresolved` reason `inferred`, detail `project` | `a_param_shadow_kills_the_name_match` |
| ts | const binding shadow: `const project = ..; project()` inside `constCase` | 1 `resolved_edge` constCase -> project, origin `corpus_unique` | no edge; `unresolved` reason `inferred` | `a_const_binding_shadow_kills_the_name_match` |
| ts | closure param shadow: `items.forEach(project => project())` inside `closureCase` | 2 `resolved_edge` rows (lambda caller `closure@216` + named mirror `closureCase` -> project), origin `corpus_unique` | no edge; `unresolved` reason `inferred` | `an_arrow_param_shadow_kills_the_name_match` |

Pre-fix wire over `free.ts` + `shadow.ts` (base d066293d binary): 4 `resolved_edge` rows to
`free.ts` (run, constCase, closureCase, closure@216), all `name_resolve`/`corpus_unique`, 0
`unresolved` rows. Post-fix wire: 0 `resolved_edge` rows, 3 `unresolved` rows reason `inferred`
detail `project` (spans 46, 129, 227 of shadow.ts).

Implementation: `TypeBinding::Shadowed` minted in `ts_receivers.rs` for a plain `Identifier`
callee bound in a new plain-locals scope (every param, every const/let binding identifier,
inserted after the declarator walk so the initializer still sees the outer binding); `ts.rs`
resolve computes `receiver` before `import_t`, the module plane declines a `Shadowed` site, the
`recv_spec` arm folds `Shadowed` into `None` so `recv_t` stays `None` and the traced-receiver
own-site arm yields no edge and no name-match fallback; `call_drops` maps `Shadowed` to
`UnresolvedReason::Inferred`.

## Gate

| check | result |
| --- | --- |
| full gate `cargo test --features cli --no-fail-fast` | 177 suites `test result: ok`, 0 failed. Last lines: `running 0 tests` / `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` (Doc-tests sprefa_extract) |
| golden_parity | `test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.47s` |
| scip ratchets | `call_resolve_scip_ratchet_ts` ok, `call_resolve_scip_ratchet_go` ok (real indexers, inside the golden_parity run) |
| receiver battery | 133: 4 passed; 134: 7 passed; 44: 1 passed; 70: 4 passed; 72: 3 passed; 73: 3 passed |

`wire_golden.jsonl` and `ts/sample.cstf.snap` needed no regen: their tests pass unchanged.

## Blocked

| item | detail |
| --- | --- |
| (empty) | |
