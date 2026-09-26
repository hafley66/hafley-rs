# ryi vs CodeQL baseline, lane Q

Implementation tip before this report: `454bc937`. CodeQL CLI 2.27.1; Rust pack 0.2.22; JavaScript pack 2.10.2; worktree ryi SHA-256 `daae7eff8ad3ddbe929556a71a4c53653687645322e4b7120b5ca081be86442a`.
Both tools read each frozen corpus. Rust queries exclude macro expansions. The four-column key is `(source file, enclosing item, target file, target name)`. Build means database creation. Times are seconds, sizes are MiB; Rust Cargo dependencies were cached. Rust type owner names were corrected and re-queried on the fresh databases after the timed runs.
Root extraction rewrites only a private copy of `ra_ap_parser-0.0.349/test_data/lexer/err/incomplete_frontmatter_before_unicode.rs`, whose `-│` makes CodeQL panic at a UTF-8 byte boundary.
The recorded timings predate the new 2048 MiB limits. Subsequent runs pass `RYI_MAX_MEM_MB=2048`, CodeQL `--ram=2048 --threads=4 -J=-Xmx2g`, and remove temporary databases on exit.

| Corpus (files) | Type agree / ryi-only / CodeQL-only | Call agree / ryi-only / CodeQL-only | ryi build / query / DB | CodeQL build / query / DB |
| --- | ---: | ---: | ---: | ---: |
| Root Rust (423) | 7126 / 13 / 1151 | 10568 / 2843 / 3191 | 18 / 1.338 / 604.7 | 109 / 873 / 1253.0 |
| sprefa Rust (552) | 1302 / 1142 / 415 | 3617 / 1818 / 605 | 9 / 0.462 / 261.4 | 53 / 38 / 296.4 |
| TypeScript (1651) | 11326 / 18600 / 6350 | 878 / 6241 / 3890 | 52 / 2.832 / 2272.1 | 87 / 31 / 676.3 |
| TypeScript after T2 (1651) | unmeasured: 2048 MiB heap cap | unmeasured: 2048 MiB heap cap | aborted / — / — | reused query planned; ryi aborted first |

Lane T2 reran the unchanged script with `RYI_CODEQL_REUSE=1` against `sprefa/v6`. Its default staged walk included 1270 files. The 1651-file direct-root run exceeded `RYI_MAX_MEM_MB=2048` with the default, two, and one extraction worker; it aborted before CodeQL. The staged rows below compare one source snapshot and reused CodeQL queries. A lane-specific copy of the freshly built ryi binary prevented concurrent builds from replacing the measured executable.

| TypeScript staged (1270) | Type agree / ryi-only / CodeQL-only | Call agree / ryi-only / CodeQL-only |
| --- | ---: | ---: |
| Before T2 | 3018 / 17235 / 4104 | 781 / 4065 / 2404 |
| After T2 | 6449 / 13804 / 673 | 2993 / 10842 / 192 |

CodeQL-only rows after T2 on the staged corpus, partitioned by source and spelling (source text classification; a row can contain more than one call site):

| Pattern | Type | Call | Sample |
| --- | ---: | ---: | --- |
| Same-file references and calls | 358 | 26 | `Page`; `sqlShape` |
| Named-import type references | 309 | 0 | `IGraphNs` |
| Cross-file type reference without named import | 6 | 0 | `Fact` |
| Cross-file member-call spelling | 0 | 163 | `execute` |
| Cross-file direct-call spelling | 0 | 3 | `buildRuleGraph` |

The blob-and-span binding reduced staged CodeQL-only type rows by 3431, primarily generated same-name peers. The private local-function binding reduced staged CodeQL-only call rows by 2212. Paired wrong-target rows pointing at generated siblings fell to 0 for both type and call. In test 181, only the new `_5_peer_a.ts` and `_6_peer_b.ts` rows were added. No existing expected row changed. The 2000 sorted TypeScript-5.9 compiler-case files completed in 0.36 s wall against the 1.1 s reference.

Causes below partition each disagreement bucket by observable pattern. Sample verdict cells in `disagreements.tsv` remain blank for human adjudication. R means ryi-only; C means CodeQL-only.

| Bucket | Cause | Count | Example | Owner |
| --- | --- | ---: | --- | --- |
| Root type R | False local target; unresolved CodeQL type | 7; 6 | `TraceEvent`; `ReadRequest` | ryi defect; out of scope |
| Root type C | Missing ryi type edge | 1151 | `TurnEvent` | ryi defect |
| Root call R | Target path mismatch; synthetic caller; unresolved CodeQL static target | 6; 1357; 1480 | `rows`; `closure@`; `lookup` | ryi defect; out of scope; codeql query defect |
| Root call C | Missing ryi call edge | 3191 | `default` | ryi defect |
| sprefa type R | Cross-fixture or standard type bound locally; unresolved fixture/module | 362; 780 | `Result`; `MoveCx` | ryi defect; out of scope |
| sprefa type C | Missing ryi type edge | 415 | `Key` | ryi defect |
| sprefa call R | Cross-fixture target; synthetic caller; unresolved CodeQL target | 56; 666; 1096 | `load_config`; `closure@`; `as_id` | ryi defect; out of scope; codeql query defect |
| sprefa call C | Missing ryi call edge | 605 | `run` | ryi defect |
| TypeScript type R | Wrong generated peer; import/alias query gap | 5127; 13473 | `IBindPlanData`; `IGenProgram` | ryi defect; codeql query defect |
| TypeScript type C | Missing or wrong ryi target; alias query mismatch | 6194; 156 | `Fact`; `IHostColumnPlan` | ryi defect; codeql query defect |
| TypeScript call R | Wrong generated peer; synthetic caller; unresolved CodeQL callee | 97; 3925; 2219 | `applyArrivals`; `closure@`; `list_at_scalar_seam` | ryi defect; out of scope; codeql query defect |
| TypeScript call C | Missing ryi call edge | 3890 | `parseArgs` | ryi defect |

Gate: `PATH="$HOME/.local/bin:$PATH" CARGO_TARGET_DIR="$HOME/.cache/boop/cargo-target" CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 RYI_MAX_MEM_MB=2048 cargo test --features cli --test 179_codeql_baseline -- --nocapture` from `crates/sprefa-extract`.

```text
running 1 test
test type_ladder_codeql_baseline ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 49.10s

```

T2 gate: `CARGO_TARGET_DIR=$HOME/.cache/boop/cargo-target CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 RYI_MAX_MEM_MB=2048 cargo test --features cli` reached `178_ryi_help` and stopped. Its help diff contained only the build hash/timestamp (`494582184e47` versus `14342bf25815`) from shared-target binary replacement. The focused `178_ryi_help` rerun passed 2/2; `181_ts_ladder` passed 1/1. Last five gate lines:

```text
    generated_clap_help_matches_captured_main

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s

error: test failed, to rerun pass `--test 178_ryi_help`
```
