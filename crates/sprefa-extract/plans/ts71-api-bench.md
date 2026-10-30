# TS 7.1 beta API vs ryi fast, 2026-10-29

Brief: `plans/ts71-api-bench.BRIEF.md`. Package `typescript@7.1.0-beta` (npm `beta` tag), sync API (`typescript/sync`, msgpack over stdio to `tsc --api`).
Baselines: `ryii` build 0.2.2 (git 27f1fbca; `--version` prints 0.2.1). Machine: darwin arm64, 12 cores, 16 GiB, node v24.15.0.

## Status

| item | state |
| --- | --- |
| ajv, codegraph-src, vite, typescript-5.9 src | 3 runs each, medians below |
| vscode src | no median: run stopped at user request ("christ my cpu"); single-run observations in the vscode section |
| hafley-rxjs, hafley-tsp | cloned and installed, not run (stopped before reaching them) |
| `ryii fast --resolve` | rejected by 0.2.2 (`unexpected argument '--resolve'`); measured `ryii --resolve` and `ryii --resolve --arms call` instead |
| slow tier (`ryii slow`, `ryi slow`) | both installed binaries exit 1: `ryi slow: TypeScript requires cargo feature ts-checker`; dropped per brief |

Resume the remaining corpora: `python3 bench/scripts/8_ts71_vs_ryi.py --runs 3 vscode hafley-rxjs hafley-tsp`, then
`python3 bench/scripts/9_ts71_report.py plans`. vscode holds the tsgo server at 3-4 GiB RSS and 350-420% CPU for 5+ minutes per run.

## Method

- `bench/scripts/7_ts71_api_bench.mjs`: one API session per run. Opens the corpus tsconfigs (`createSnapshot{openProjects}`), takes the
  project-owned `.ts/.tsx/.mts/.cts` files under the corpus root (no `node_modules`), then:
  bulk phases over every file (AST fetch + walk; `getSymbolAtPosition` batched per file on every call/new callee name; `getAliasedSymbol`
  on alias hits; `getTypeAtPosition` on call names; `getSymbolAtPosition` on every identifier; `getSemanticDiagnostics` per project),
  then one task per API method over a seeded 200-item sample (smaller caps on the slow LS calls), then `createProgram`, `emit`, build orchestrator.
  Every RPC is timed at `Client.apiRequest`/`apiRequestBinary`/`batchRequests` and attributed to its task.
- Peak RSS per task: a worker thread polls the tsgo server (`ps -o rss`) and the node client (`process.memoryUsage().rss`) every 25 ms while the
  main thread blocks in sync RPCs, plus one sample at task end. Tasks shorter than 25 ms carry the end sample. The server column is the memory
  answer; the tsgo process holds the checker. The sampler adds one `ps` exec per 25 ms on another core.
- Order matters: the checker caches across phases, so later tasks run warm. Bulk phases run first in the listed order.
- `ts71_call_resolution_ms` = sum of `init_spawn`, `snapshot_open_projects`, `program_source_file_names`, `bulk_ast_fetch_and_walk`,
  `bulk_call_symbols`, `bulk_call_alias_targets` inside the node process; it excludes node startup and module import (~60 ms).
  `ryi_*_ms` is whole-process wall from Python `perf_counter`, same file list on argv, `--root .`, `RUST_LOG=error`.
- `peak_rss_mb` for ryi rows is `/usr/bin/time -l` maximum resident set size. For the API process that figure equals the tsgo server peak
  (`time -l` reports the larger child), so the API table splits server and client from the sampler.
- Dependencies installed with `--ignore-scripts` in every clone (npm or pnpm per lockfile); corpus clones are plain untracked checkouts under
  `bench/repos/` (gitignored), no submodules. `typescript-5.9` = local clone of `release-5.9` at 7e133bea, `src/` leaf tsconfigs.
- Data: `bench/repos/ts71/ts71.db` (tables `process`, `task`, `rpc`, `counts`, `record_kind`, `api_method`, `corpus`), per-run JSON and stderr beside it.
- Grids: `plans/ts71-api-bench.tasks.html` (every task x corpus: wall_ms, server/client peak and end RSS, errors),
  `plans/ts71-api-bench.methods.html` (every RPC method x corpus: calls, total_ms, median_call_ms, peak server RSS of issuing tasks; never-called methods listed as `(none)`).

## Call resolution: ryi vs TS 7.1 API, median of runs

| corpus | files | ryi_resolve_call_ms | ryi_resolve_call_peak_rss_mb | ts71_call_resolution_ms | ts71_server_peak_rss_mb |
| --- | --- | --- | --- | --- | --- |
| typescript-5.9 | 601 | 1,164.8 | 513.8 | 2,588.5 | 643.8 |
| codegraph-src | 292 | 388.1 | 154.6 | 821.7 | 311.9 |
| vite | 157 | 179.5 | 81.5 | 530.2 | 329 |
| ajv | 106 | 84.8 | 41.0 | 120.6 | 112.3 |

## Corpora

| corpus | commit | projects | files | ast_nodes |
| --- | --- | --- | --- | --- |
| typescript-5.9 | 7e133bea12 | 13 | 601 | 1,691,449 |
| codegraph-src | e63fe2ec5b | 5 | 292 | 568,722 |
| vite | e57f4da9c6 | 6 | 157 | 191,694 |
| ajv | f177fe3234 | 1 | 106 | 54,316 |

| corpus | call_sites | identifiers | ts71_runs | ts71_exit_codes |
| --- | --- | --- | --- | --- |
| typescript-5.9 | 99,542 | 670,310 | 3 | 0,0,0 |
| codegraph-src | 29,762 | 188,683 | 3 | 0,0,0 |
| vite | 9,611 | 65,144 | 3 | 0,0,0 |
| ajv | 2,159 | 18,707 | 3 | 0,0,0 |

## `ryi_fast` process, median of runs with exit 0

| corpus | wall_ms | peak_rss_mb | stdout_records | exit_codes |
| --- | --- | --- | --- | --- |
| typescript-5.9 | 1,619.8 | 1,029.1 | 713,943 | 0,0,0 |
| codegraph-src | 477.9 | 351.1 | 180,741 | 0,0,0 |
| vite | 223.5 | 147.8 | 67,332 | 0,0,0 |
| ajv | 94.6 | 53.2 | 21,552 | 0,0,0 |

## `ryi_resolve` process, median of runs with exit 0

| corpus | wall_ms | peak_rss_mb | stdout_records | exit_codes |
| --- | --- | --- | --- | --- |
| typescript-5.9 | 1,237.9 | 544.0 | 255,973 | 0,0,0 |
| codegraph-src | 393.5 | 157.5 | 60,003 | 0,0,0 |
| vite | 179.7 | 79.8 | 21,209 | 0,0,0 |
| ajv | 89.1 | 40.7 | 7,605 | 0,0,0 |

## `ryi_resolve_call` process, median of runs with exit 0

| corpus | wall_ms | peak_rss_mb | stdout_records | exit_codes |
| --- | --- | --- | --- | --- |
| typescript-5.9 | 1,164.8 | 513.8 | 227,099 | 0,0,0 |
| codegraph-src | 388.1 | 154.6 | 55,997 | 0,0,0 |
| vite | 179.5 | 81.5 | 19,163 | 0,0,0 |
| ajv | 84.8 | 41.0 | 6,236 | 0,0,0 |

## `ryi_slow` process, median of runs with exit 0

| corpus | wall_ms | peak_rss_mb | stdout_records | exit_codes |
| --- | --- | --- | --- | --- |
| typescript-5.9 |  |  |  | 1,1,1 |
| codegraph-src |  |  |  | 1,1,1 |
| vite |  |  |  | 1,1,1 |
| ajv |  |  |  | 1,1,1 |

## TS 7.1 API whole process (bulk phases + every method task + build), median of runs

| corpus | wall_ms | node_peak_rss_mb | tsgo_server_peak_rss_mb | node_client_peak_rss_mb |
| --- | --- | --- | --- | --- |
| typescript-5.9 | 17,927.4 | 1,518.1 | 1,518.1 | 1,018.7 |
| codegraph-src | 6,065.1 | 602.7 | 602.7 | 499.0 |
| vite | 6,214.7 | 1,696.9 | 1,696.9 | 353.3 |
| ajv | 2,315.0 | 226.0 | 226.0 | 225.2 |

## TS 7.1 API bulk phases over every corpus file, median of runs (run order = row order)


### typescript-5.9

| phase | wall_ms | server_peak_rss_mb | client_peak_rss_mb | server_end_rss_mb |
| --- | --- | --- | --- | --- |
| init_spawn | 6.903 | 0 | 68.1 | 0 |
| snapshot_open_projects | 123.9 | 295.5 | 74.1 | 295.5 |
| program_source_file_names | 3.709 | 296.2 | 76 | 296.2 |
| bulk_ast_fetch | 172.1 | 446.5 | 447.7 | 446.5 |
| bulk_ast_walk | 239.2 | 446.5 | 447.7 | 446.5 |
| bulk_call_symbols | 1,482.8 | 643.6 | 597.9 | 643.6 |
| bulk_call_alias_targets | 559.5 | 643.8 | 616.2 | 643.8 |
| bulk_call_types | 778.2 | 719.7 | 620.7 | 719.7 |
| bulk_identifier_symbols | 6,735.9 | 1,321.9 | 937.3 | 1,321.9 |
| bulk_semantic_diagnostics | 1,175.6 | 1,322.3 | 823.0 | 1,322.3 |

### codegraph-src

| phase | wall_ms | server_peak_rss_mb | client_peak_rss_mb | server_end_rss_mb |
| --- | --- | --- | --- | --- |
| init_spawn | 7.253 | 0 | 68.5 | 0 |
| snapshot_open_projects | 62.2 | 151.0 | 73.3 | 151.0 |
| program_source_file_names | 1.211 | 151.2 | 73.8 | 151.2 |
| bulk_ast_fetch | 91.7 | 225.3 | 222.1 | 225.3 |
| bulk_ast_walk | 89.0 | 225.3 | 222.1 | 225.3 |
| bulk_call_symbols | 501.1 | 311.8 | 280.6 | 311.8 |
| bulk_call_alias_targets | 69.5 | 311.9 | 292.6 | 311.9 |
| bulk_call_types | 257.4 | 355.6 | 307.3 | 355.6 |
| bulk_identifier_symbols | 1,833.8 | 429.9 | 459.6 | 429.9 |
| bulk_semantic_diagnostics | 386.8 | 430.2 | 460.9 | 430.2 |

### vite

| phase | wall_ms | server_peak_rss_mb | client_peak_rss_mb | server_end_rss_mb |
| --- | --- | --- | --- | --- |
| init_spawn | 6.940 | 0 | 68.5 | 0 |
| snapshot_open_projects | 90.8 | 179.4 | 73.5 | 179.4 |
| program_source_file_names | 2.163 | 180.1 | 74.9 | 180.1 |
| bulk_ast_fetch | 27.9 | 211 | 124.6 | 211 |
| bulk_ast_walk | 37.8 | 211 | 124.6 | 211 |
| bulk_call_symbols | 318.7 | 328.9 | 158.6 | 328.9 |
| bulk_call_alias_targets | 46.7 | 329 | 168.6 | 329 |
| bulk_call_types | 90.1 | 374.3 | 171.7 | 374.3 |
| bulk_identifier_symbols | 695.3 | 430.5 | 244.9 | 430.5 |
| bulk_semantic_diagnostics | 340.2 | 430.7 | 247.7 | 430.7 |

### ajv

| phase | wall_ms | server_peak_rss_mb | client_peak_rss_mb | server_end_rss_mb |
| --- | --- | --- | --- | --- |
| init_spawn | 7.020 | 0 | 68.4 | 0 |
| snapshot_open_projects | 32.1 | 75.2 | 73.4 | 75.2 |
| program_source_file_names | 0.465 | 75.3 | 74.1 | 75.3 |
| bulk_ast_fetch | 11.5 | 83.5 | 91.9 | 83.5 |
| bulk_ast_walk | 13.8 | 83.5 | 91.9 | 83.5 |
| bulk_call_symbols | 44.8 | 112.2 | 100.8 | 112.2 |
| bulk_call_alias_targets | 9.995 | 112.3 | 101.5 | 112.3 |
| bulk_call_types | 19.1 | 118.8 | 102.0 | 118.8 |
| bulk_identifier_symbols | 177.9 | 140.8 | 130.8 | 140.8 |
| bulk_semantic_diagnostics | 39.0 | 141.0 | 130.9 | 141.0 |

## Call counts: ryi --resolve --arms call vs TS 7.1 getSymbolAtPosition on call names

| corpus | ts71_call_sites | ts71_symbol_found | ts71_decl_in_corpus | ts71_semantic_diagnostics |
| --- | --- | --- | --- | --- |
| typescript-5.9 | 99,542 | 99,398 | 85,103 | 58 |
| codegraph-src | 29,762 | 27,567 | 11,513 | 1,457 |
| vite | 9,611 | 9,562 | 3,844 | 42 |
| ajv | 2,159 | 2,159 | 1,735 | 9 |

| corpus | ryi_call_site | ryi_resolved_edge | ryi_unresolved |
| --- | --- | --- | --- |
| typescript-5.9 | 100,185 | 90,030 | 17,175 |
| codegraph-src | 29,870 | 10,480 | 12,785 |
| vite | 9,720 | 3,465 | 3,782 |
| ajv | 2,607 | 1,851 | 435 |

## Method coverage

| metric | value |
| --- | --- |
| APIMethodInfo methods | 177 |
| methods called on at least one corpus | 163 |
| methods never called | getBaseTypeOfType, getCachedSourceFile, getCheckTypeOfType, getConstraintOfType, getExportSymbolOfSymbol, getExtendsTypeOfType, getFalseTypeOfConditionalType, getIndexTypeOfType, getLocalTypeParametersOfType, getNameTypeOfMappedType, getObjectTypeOfType, getOuterTypeParametersOfType, getRegularTypeOfType, getTrueTypeOfConditionalType |

## Task errors (any run)

| corpus | task | errors | inputs | first_error |
| --- | --- | --- | --- | --- |
| ajv | api.getSymbol | 18 | 200 | api: client error: declaration node index 2191 has no binder symbol |
| codegraph-src | api.getSymbol | 5 | 200 | api: client error: declaration node index 21190 has no binder symbol |
| typescript-5.9 | api.getSymbol | 5 | 200 | api: client error: declaration node index 21409 has no binder symbol |
| vite | api.getSymbol | 14 | 200 | api: client error: declaration node index 840 has no binder symbol |
| typescript-5.9 | debug.cpuProfile | 1 | 1 | api: client error: source file not found: /Users/chrishafley/projects/hafley-rs-wt/ryi-reconcile/crates/sprefa |
| ajv | formatNodeForInsertion | 5 | 20 | panic: Debug failure. False expression. |
| codegraph-src | formatNodeForInsertion | 3 | 20 | panic: Debug failure. False expression. |
| typescript-5.9 | formatNodeForInsertion | 3 | 20 | panic: Debug failure. False expression. |
| vite | formatNodeForInsertion | 1 | 20 | panic: Debug failure. False expression. |
| ajv | getCurrentLanguageServerSnapshot | 1 | 1 | api: client error: getCurrentLanguageServerSnapshot requires an LSP-connected API session |
| codegraph-src | getCurrentLanguageServerSnapshot | 1 | 1 | api: client error: getCurrentLanguageServerSnapshot requires an LSP-connected API session |
| typescript-5.9 | getCurrentLanguageServerSnapshot | 1 | 1 | api: client error: getCurrentLanguageServerSnapshot requires an LSP-connected API session |
| vite | getCurrentLanguageServerSnapshot | 1 | 1 | api: client error: getCurrentLanguageServerSnapshot requires an LSP-connected API session |
| vite | getImmediateAliasedSymbol | 1 | 200 | panic: Unexpected nil in getImmediateAliasedSymbol |
| vite | isTypeAssignableTo | 179 | 200 | api: client error: type handle 86 not found in project registry |
| typescript-5.9 | transpileDeclaration | 1 | 50 | panic: Debug failure. False expression: Output generation failed |
| vite | transpileDeclaration | 2 | 50 | panic: Debug failure. False expression: Output generation failed |
| typescript-5.9 | transpileDeclarationFromFile | 1 | 50 | panic: Debug failure. False expression: Output generation failed |
| vite | transpileDeclarationFromFile | 2 | 50 | panic: Debug failure. False expression: Output generation failed |
| typescript-5.9 | transpileModule | 1 | 50 | panic: Debug failure. False expression: Output generation failed |
| vite | transpileModule | 2 | 50 | panic: Debug failure. False expression: Output generation failed |
| typescript-5.9 | transpileModuleFromFile | 1 | 50 | panic: Debug failure. False expression: Output generation failed |
| vite | transpileModuleFromFile | 2 | 50 | panic: Debug failure. False expression: Output generation failed |

## Selected method tasks, median of runs


### typescript-5.9

| task | inputs | wall_ms | ms_per_input | server_peak_rss_mb |
| --- | --- | --- | --- | --- |
| getSymbolAtPosition | 200 | 5.652 | 0.028 | 1,322.3 |
| getTypeAtPosition | 200 | 4.265 | 0.021 | 1,322.3 |
| getResolvedSignature | 200 | 4.252 | 0.021 | 1,322.4 |
| typeToString | 200 | 3.588 | 0.018 | 1,322.6 |
| isTypeAssignableTo | 200 | 4.282 | 0.021 | 1,322.6 |
| getReferencesToSymbolInFile | 50 | 67.9 | 1.358 | 1,322.5 |
| getReferencedSymbolsForNode | 20 | 122.5 | 6.127 | 1,330.5 |
| getSignatureUsage | 20 | 22.6 | 1.129 | 1,330.7 |
| getSymbolsInScope | 20 | 231.7 | 11.6 | 1,322.4 |
| getCompletionsAtPosition | 50 | 29.9 | 0.598 | 1,327.0 |
| getImportEditsForSymbols | 20 | 198.0 | 9.899 | 1,332.4 |
| getSemanticDiagnostics | 200 | 2.900 | 0.015 | 1,322.6 |
| getJavaScriptEmit | 200 | 1,104.4 | 5.522 | 1,324.3 |
| runWithTemporaryFileUpdate+semanticDiagnostics | 5 | 352.8 | 70.6 | 1,514.6 |
| api.batch(getSymbolAtPosition) | 1 | 4.817 | 4.817 | 1,514.6 |
| createProgram | 1 | 4.829 | 4.829 | 1,514.6 |
| emit | 1 | 55.9 | 55.9 | 1,516.3 |
| build | 1 | 452.1 | 452.1 | 1,518.1 |

### codegraph-src

| task | inputs | wall_ms | ms_per_input | server_peak_rss_mb |
| --- | --- | --- | --- | --- |
| getSymbolAtPosition | 200 | 5.863 | 0.029 | 430.2 |
| getTypeAtPosition | 200 | 4.513 | 0.023 | 430.2 |
| getResolvedSignature | 200 | 3.750 | 0.019 | 430.2 |
| typeToString | 200 | 4.114 | 0.021 | 430.5 |
| isTypeAssignableTo | 200 | 2.660 | 0.013 | 430.4 |
| getReferencesToSymbolInFile | 50 | 11.3 | 0.225 | 430.4 |
| getReferencedSymbolsForNode | 20 | 77.5 | 3.876 | 484.8 |
| getSignatureUsage | 20 | 6.468 | 0.323 | 486.7 |
| getSymbolsInScope | 20 | 9.568 | 0.478 | 430.2 |
| getCompletionsAtPosition | 50 | 26.6 | 0.532 | 481.9 |
| getImportEditsForSymbols | 20 | 70.4 | 3.521 | 529.2 |
| getSemanticDiagnostics | 200 | 6.371 | 0.032 | 430.5 |
| getJavaScriptEmit | 200 | 474.1 | 2.371 | 440.6 |
| runWithTemporaryFileUpdate+semanticDiagnostics | 5 | 24.3 | 4.861 | 600.6 |
| api.batch(getSymbolAtPosition) | 1 | 4.516 | 4.516 | 600.6 |
| createProgram | 1 | 2.035 | 2.035 | 600.6 |
| emit | 1 | 0.471 | 0.471 | 602.3 |
| build | 1 | 115.7 | 115.7 | 602.7 |

### vite

| task | inputs | wall_ms | ms_per_input | server_peak_rss_mb |
| --- | --- | --- | --- | --- |
| getSymbolAtPosition | 200 | 5.143 | 0.026 | 430.7 |
| getTypeAtPosition | 200 | 4.341 | 0.022 | 430.8 |
| getResolvedSignature | 200 | 4.252 | 0.021 | 430.8 |
| typeToString | 200 | 6.179 | 0.031 | 431.7 |
| isTypeAssignableTo | 200 | 3.917 | 0.020 | 431.6 |
| getReferencesToSymbolInFile | 50 | 4.745 | 0.095 | 431.6 |
| getReferencedSymbolsForNode | 20 | 40.0 | 1.999 | 1,187.7 |
| getSignatureUsage | 20 | 24.8 | 1.239 | 1,215.9 |
| getSymbolsInScope | 20 | 123.5 | 6.176 | 431.3 |
| getCompletionsAtPosition | 50 | 35.9 | 0.718 | 1,143.2 |
| getImportEditsForSymbols | 20 | 135.7 | 6.787 | 1,559.1 |
| getSemanticDiagnostics | 157 | 2.305 | 0.015 | 431.7 |
| getJavaScriptEmit | 157 | 352.2 | 2.243 | 1,028.6 |
| runWithTemporaryFileUpdate+semanticDiagnostics | 5 | 37.9 | 7.574 | 1,693.0 |
| api.batch(getSymbolAtPosition) | 1 | 4.405 | 4.405 | 1,693.4 |
| createProgram | 1 | 14.0 | 14.0 | 1,694.5 |
| emit | 1 | 0.179 | 0.179 | 1,696.4 |
| build | 1 | 41.6 | 41.6 | 1,696.9 |

### ajv

| task | inputs | wall_ms | ms_per_input | server_peak_rss_mb |
| --- | --- | --- | --- | --- |
| getSymbolAtPosition | 200 | 5.103 | 0.026 | 141.0 |
| getTypeAtPosition | 200 | 4.219 | 0.021 | 141.0 |
| getResolvedSignature | 200 | 3.823 | 0.019 | 141.0 |
| typeToString | 200 | 5.317 | 0.027 | 165.0 |
| isTypeAssignableTo | 200 | 2.625 | 0.013 | 165.0 |
| getReferencesToSymbolInFile | 50 | 2.923 | 0.058 | 164.9 |
| getReferencedSymbolsForNode | 20 | 22.3 | 1.115 | 175.0 |
| getSignatureUsage | 20 | 6.755 | 0.338 | 175.0 |
| getSymbolsInScope | 20 | 135.5 | 6.774 | 164.6 |
| getCompletionsAtPosition | 50 | 7.168 | 0.143 | 170.3 |
| getImportEditsForSymbols | 20 | 98.9 | 4.943 | 218.2 |
| getSemanticDiagnostics | 106 | 1.534 | 0.014 | 165.1 |
| getJavaScriptEmit | 106 | 62.4 | 0.589 | 166.1 |
| runWithTemporaryFileUpdate+semanticDiagnostics | 5 | 11.6 | 2.327 | 221.2 |
| api.batch(getSymbolAtPosition) | 1 | 3.976 | 3.976 | 221.3 |
| createProgram | 1 | 6.135 | 6.135 | 221.8 |
| emit | 1 | 38.9 | 38.9 | 223.5 |
| build | 1 | 54.6 | 54.6 | 225.5 |

## Server RSS jumps by task (run 1, peak minus previous task's end RSS)

| corpus | task | server_peak_rss_mb | jump_mb |
| --- | --- | --- | --- |
| vite | getGlobalDiagnostics | 934 | 490 |
| vite | getImportAdderEdits | 1,653 | 301 |
| vite | snapshot_open_projects | 178 | 178 |
| typescript-5.9 | bulk_identifier_symbols | 1,346 | 620 |
| typescript-5.9 | snapshot_open_projects | 295 | 295 |
| typescript-5.9 | bulk_call_symbols | 641 | 206 |
| codegraph-src | snapshot_open_projects | 151 | 151 |
| codegraph-src | getReferencedSymbolsForNode | 547 | 91 |
| ajv | snapshot_open_projects | 80 | 80 |
| ajv | getImportEditsForSymbols | 220 | 50 |

Server RSS does not fall between tasks on any corpus or run: no task ends more than 5 MB below the previous task's end RSS until `close` (tasks grid, `server_end_rss_mb`).

## vscode, single-run observations (not medians)

11,113 files, `src/tsconfig.json`.

| run | tool | wall_ms | peak_rss_mb | exit_code | note |
| --- | --- | --- | --- | --- | --- |
| first pass | ryi_fast | 13,585 | 7,476 | 137 | `memcap: ryii footprint 8.14 GiB over cap 8.00 GiB (HAFLEY_MEMCAP_GB); exiting 137` |
| first pass | ryi_resolve | 16,322 | 6,414 | 0 | |
| first pass | ryi_resolve_call | 15,012 | 6,242 | 0 | |
| first pass | ts71_api | 281,324 | 3,928 | -6 | node V8 abort; client kept every sampled AST node (fixed with reservoirs + `--max-old-space-size=10240`) |
| second pass | ts71_api | stopped | tsgo 3.0-3.9 GiB sampled by `ps` | killed | reached `bulk_identifier_symbols` (bulk phases through `bulk_call_types` done) when stopped |

## API surface the timings cover (data for "what can we do with it")

| capability | API calls | measured cost on typescript-5.9 (warm, median) |
| --- | --- | --- |
| checker symbol/type at any offset, batched per file | `getSymbolAtPosition(file, positions[])`, `getTypeAtPosition` | bulk over 99,542 call names: 1,483 ms; over 670,310 identifiers: 6,736 ms, server RSS +620 MB |
| alias chasing to declaring node | `getAliasedSymbol`, `Symbol.declarations` (`NodeHandle` path + index) | 559 ms for every alias hit |
| type relations | `isTypeAssignableTo`, `getBaseTypes`, `getPropertiesOfType`, `typeToString` | sub-ms per call (see Selected method tasks) |
| per-file references, project-wide references | `getReferencesToSymbolInFile`, `getReferencedSymbolsForNode` | 1.4 ms per call; 6.1 ms per call |
| signature call sites | `getSignatureUsage` | 1.1 ms per call |
| in-memory edit then re-check | `runWithTemporaryFileUpdate` + `getSemanticDiagnostics` | 70.6 ms per edited file, server RSS +190 MB |
| import edits for a symbol | `getImportEditsForSymbols`, `getImportAdderEdits` | 9.9 ms per call |
| emit without touching disk | `getJavaScriptEmit`, `getDeclarationEmit`, `emitToString` | 5.5 ms per file JS emit |
| batching many requests in one round trip | `api.batch(...gen)` | 200 `getSymbolAtPosition` in 4.8 ms vs 5.7 ms sequential |

Not callable from a plain API session: `getCurrentLanguageServerSnapshot` (requires LSP-connected session). Beta panics recorded under Task errors:
`formatNodeForInsertion` (Debug failure), `transpile*` on some files (Output generation failed), `getImmediateAliasedSymbol` (unexpected nil);
`isTypeAssignableTo` across projects fails with `type handle not found in project registry` (vite, 179/200: the target type came from another project).
