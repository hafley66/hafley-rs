# ryi TypeScript versus CodeQL, lane T3

Corpus: frozen `sprefa/v6`, 1651 TypeScript files, plus package and tsconfig files. Key: `(source file, enclosing item, target file, target name)`. CodeQL 2.27.1 uses the reused database and fair JavaScript query from T2. Ryi ran with `RYI_MAX_MEM_MB=2048`.

| State | Type agree / ryi-only / CodeQL-only | Call agree / ryi-only / CodeQL-only |
| --- | ---: | ---: |
| Before | 16870 / 13473 / 806 | 4451 / 22786 / 416 |
| After | 17350 / 13488 / 326 | 4633 / 23040 / 234 |

Type: 495 added, 0 removed, including 480 new agreements and 15 new ryi-only rows. Call: 436 added, 0 removed, including 182 new agreements and 254 new ryi-only rows.

Named relative imports were present in the module index; type references inside class, interface, constructor, and object method signatures did not reach fast type edge candidates. Class methods now record candidates. The module walk collects interface, constructor, and object method signatures; `type_facts` joins only local TypeF declarations or module-bound imports. The resolve CLI retains its pinned type edge set. Local `link:` package dependencies now resolve through the existing oxc resolver within the staged corpus; this exposes imported receiver types and their method targets.

Ladder additions in test 181, with no changed or deleted expected rows:

- `_0_types.ts ping`: `param Base`, `returns Base`; `_4_nested.ts run`: `generic Base`, `param Box`, `returns Base`. Fast-only class method signature refs; slow and phase-one output retain their baseline rows.
- `packages/engine/0_types.ts execute`: `param Result`, `returns Result`. Interface method signature refs.
- `packages/consumer/0_use.ts invoke` and `invokeHolder`: `call execute`. Typed receiver methods across a linked package.
- `packages/consumer/0_use.ts Holder field EnginePort`, `Runner field EnginePort`, `constructor param EnginePort`, `forward param EnginePort`, `invoke param EnginePort`, `invokeHolder param Holder`, `use param EnginePort`, `forward returns EnginePort`, `use returns EnginePort`. Imported, local, constructor, and object method type refs.

[Row audit](2026-09-26-ryi-ts-round3.T3-AUDIT.tsv): 20 pre-existing ryi-only type samples, 20 new ryi-only call samples, and all 15 new ryi-only type rows. Each row has a source, target, evidence, and verdict. The 15 new type rows resolve declared types re-exported by `tsv2/runtime/types.ts`; the 20 sampled call rows resolve `seam.runner` methods declared in the linked engine package.

Residual CodeQL-only: type 100 same-file, 226 cross-file; call 149 same-file, 85 cross-file. Same-file type includes 28 `T` generic targets; same-file call includes 112 `bind_args` rows under the duplicate-name guard. Of the cross-file type rows, 212 name `tsv2/runtime/types.ts`; 165 target its `SqlStatement` re-export, while ryi binds the declaration in `sprefa-store/js/src/engine/types.ts`.

Speed: sorted first 2000 TypeScript-5.9 compiler `*.ts`, four workers, streamed `ryi fast`. The 0.352 s limit is +10% of T2's 0.32 s. Final-binary warm minimum was 0.342 s; seven-run medians were 0.359 and 0.358 s under concurrent CPU load. Paired old/new runs under that load were 0.400–0.539 / 0.381–0.449 s. The minimum meets the limit; the medians exceed it.

Gate: `cd crates/sprefa-extract && cargo test --features cli` passed with `CARGO_TARGET_DIR=$HOME/.cache/boop/cargo-target`, `CARGO_BUILD_JOBS=4`, `RUST_TEST_THREADS=4`, and `RYI_MAX_MEM_MB=2048`. Focused test 181 (1/1), resolve CLI (7/7), v5 golden parity (11/11), and Rust type ratchet (2/2 plus one pre-existing ignored test) also passed with this lane's preserved binary. Earlier full-gate attempts failed when another worktree replaced the shared `debug/ryi` during the run; the final run passed.
