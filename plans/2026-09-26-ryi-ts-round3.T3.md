# ryi TypeScript versus CodeQL, lane T3

Corpus: frozen `sprefa/v6`, 1651 TypeScript files, plus package and tsconfig files. Key: `(source file, enclosing item, target file, target name)`. CodeQL 2.27.1 uses the reused database and fair JavaScript query from T2. Ryi ran with `RYI_MAX_MEM_MB=2048`.

| State | Type agree / ryi-only / CodeQL-only | Call agree / ryi-only / CodeQL-only |
| --- | ---: | ---: |
| Before | 16870 / 13473 / 806 | 4451 / 22786 / 416 |
| After prior validated step | 17350 / 13488 / 326 | 4633 / 23040 / 234 |

Type: 495 added, 0 removed, including 480 new agreements and 15 new ryi-only rows. Call: 436 added, 0 removed, including 182 new agreements and 254 new ryi-only rows.

Named relative imports were present in the module index; type references inside class, interface, constructor, and object method signatures did not reach fast type edge candidates. Class methods now record candidates. The module walk collects interface, constructor, and object method signatures; `type_facts` joins only local TypeF declarations or module-bound imports. Local `link:` package dependencies now resolve through the existing oxc resolver within the staged corpus; this exposes imported receiver types and their method targets.

Earlier ladder additions in test 181:

- `_0_types.ts ping`: `param Base`, `returns Base`; `_4_nested.ts run`: `generic Base`, `param Box`, `returns Base`. Fast-only class method signature refs; slow and phase-one output retain their baseline rows.
- `packages/engine/0_types.ts execute`: `param Result`, `returns Result`. Interface method signature refs.
- `packages/consumer/0_use.ts invoke` and `invokeHolder`: `call execute`. Typed receiver methods across a linked package.
- `packages/consumer/0_use.ts Holder field EnginePort`, `Runner field EnginePort`, `constructor param EnginePort`, `forward param EnginePort`, `invoke param EnginePort`, `invokeHolder param Holder`, `use param EnginePort`, `forward returns EnginePort`, `use returns EnginePort`. Imported, local, constructor, and object method type refs.

Review follow-up, untested after the code-only order: `duplicates/b/1_main.ts useShared -> duplicates/b/0_shared.ts shared` checks that the import's path wins when two files share one content blob. `_21_local_peer.ts useLocalHelper -> helper` checks that an unrelated exported peer cannot suppress a same-file lexical call. Existing `_15_private.ts usePrivate -> clashPriv` and `_17_export.ts useExported -> clashPub` change from `-s` to `fs` for the same reason. The `resolve_type_arm_streams_resolved_type_edges` pin `tests/fixtures/resolve/5_resolved_type_edges.jsonl` adds `scaled -> Vec` (`param`) and `scaled -> Vec2` (`returns`), both same-file class method signature references. Resolve and fast now share the signature path. The `ping -> Base` parameter and return rows remain `f-`: the committed SCIP index has no method-signature type occurrences for them.

The committed `ts_ladder/index.scip` predates the five linked-package rows and the new fixtures. `tsconfig.json` now maps `ladder-engine/*` to the engine source for the next `scip-typescript` regeneration. Its slow marks and the new ladder expectations require regeneration and verification; regeneration was withheld under the later code-only instruction. No post-follow-up row counts or timing were run.

[Row audit](2026-09-26-ryi-ts-round3.T3-AUDIT.tsv): 20 pre-existing ryi-only type samples, 20 new ryi-only call samples, and all 15 new ryi-only type rows. Each row has a source, target, evidence, and verdict. The 15 new type rows resolve declared types re-exported by `tsv2/runtime/types.ts`; the 20 sampled call rows resolve `seam.runner` methods declared in the linked engine package.

Residual CodeQL-only: type 100 same-file, 226 cross-file; call 149 same-file, 85 cross-file. Same-file type includes 28 `T` generic targets; same-file call includes 112 `bind_args` rows under the duplicate-name guard. Of the cross-file type rows, 212 name `tsv2/runtime/types.ts`; 165 target its `SqlStatement` re-export, while ryi binds the declaration in `sprefa-store/js/src/engine/types.ts`.

Speed: sorted first 2000 TypeScript-5.9 compiler `*.ts`, four workers, streamed `ryi fast`. The 0.352 s limit is +10% of T2's 0.32 s. Final-binary warm minimum was 0.342 s; seven-run medians were 0.359 and 0.358 s under concurrent CPU load. Paired old/new runs under that load were 0.400–0.539 / 0.381–0.449 s. The minimum meets the limit; the medians exceed it.

Prior gate: `cd crates/sprefa-extract && cargo test --features cli` passed before this review follow-up with a shared cargo target. It does not validate the current tree. Future gates need `CARGO_TARGET_DIR=$HOME/.cache/boop/lane-targets/feature-ryi-ts-round3`, `CARGO_BUILD_JOBS=4`, `RUST_TEST_THREADS=4`, and `RYI_MAX_MEM_MB=2048`; remove that lane target after use.
