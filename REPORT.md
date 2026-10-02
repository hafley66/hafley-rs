# ryi TS slow lane, 2026-10-02

All code and dogfood cases are committed. Build, test, and runtime results are unverified. The user's later instruction stopped parallel builds/tests and deferred gates to the coordinator. No cargo, ryii, tsc, vitest, or dogfood execution was started in this lane; no installs or pushes were performed.

Corpus prepared outside the repository at `/Users/chrishafley/projects/rxjs-corpus-feature-ryi-ts-slow`, detached at d0802620. The protected corpus directories were read only.

| Case | Before (plan measurement) | After (implemented, unverified) | Commits |
|---|---|---|---|
| D14 | Slow misses the 12 cache.render sites at lines 7,8,10,11,12,14,15,28,29,30,31,33. | Checker execution survives missing SCIP indexes. Targeted references open all supplied TS files and seed from extracted declarations. Existing `slow` and `--resolve --ts-checker` CLI outputs append tsgo destinations joined to raw call/type spans. D14.sh asserts all 12 checker-bound sites in graph, resolve, and slow SQLite facts. | 1fab89cb |
| D15 | Missing ts-checker can exit 0 with no slow edges, including an early no-index return. | Slow TS validates the feature before index discovery or targeted resolution, then exits non-zero naming ts-checker. D15.sh checks package/corpus roots and existing from/call-path/callers commands using a cli-only binary. Added missing-feature CLI regression test; corrected the case to the existing single-seed call-path syntax. | 71669f2f, a1d1da29 |
| D17 | Fast method rename exits 6 with 286 runtime-seat lines. | Typed method plans retain their existing behavior. Unresolved method receivers produce one Refused diagnostic naming --slow. Property declarations retain their existing dynamic-seat behavior. D17.sh asserts non-zero exit, one stderr line, no stdout plan, and unchanged source. | bc016456 |
| J01 | No tested slow JSX attribute-to-props declaration binding. | Existing resolved_edge binds the component. CST attribute identifier spans feed tsgo definition requests; symbol and occurrence def/ref rows bind a qualified FooProps.bar declaration. J01.sh asserts Foo, bar, exact UTF-8 source offsets, and the props member declaration in resolve and slow SQLite outputs. | 4718ffbb |

D14 changes facts and existing command behavior. No SQL analyses or graph walkers were added. No CLI commands, flags, or output variants were added.

J01 depends on the fast `jsx_element` and `jsx_attribute` record contract from `feature-ryi-ts-rtkq-jsx`, commit 826a1a05. That commit was not cherry-picked into this lane. Slow code reads existing CST facts and emits existing symbol/occurrence/resolved_edge variants; no fast JSX files were edited. A props reference uses a shared symbol of the form `tsgo <declaration-path>#FooProps.bar@<start>:<end>`; its source span covers the JSX attribute name, and the same symbol's def row identifies the member declaration.

The tsgo session first uses the crate's existing ts7 compiler installation, then a project/ancestor `node_modules/typescript/bin/tsc`. This worktree's ts7/node_modules is absent. No installation was attempted; the corpus links the already-installed project dependencies.

Coordinator gates still required, serialized with CARGO_BUILD_JOBS=4:

1. Merge the fast JSX record contract before J01.
2. In crates/sprefa-extract, run `cargo test --features cli`. Known pre-existing exceptions are `golden_parity::ported_facets_match_v5` and `golden_parity::rust_doc_parity`; no new-failure claim is made here.
3. Preserve the cli-only debug ryii path as RYII_NO_TS_CHECKER for D15.
4. Run `cargo build --release --bin ryii --features cli,ts-checker,typespec`.
5. Run `RYII=<release ryii> RYII_NO_TS_CHECKER=<cli-only ryii> CORPUS=/Users/chrishafley/projects/rxjs-corpus-feature-ryi-ts-slow dogfood/ts/run.sh D14 D15 D17 J01`.

The scripts and Rust changes remain unrun. Runtime checker behavior, compilation, crate parity, and all four expected case outcomes require these gates.
