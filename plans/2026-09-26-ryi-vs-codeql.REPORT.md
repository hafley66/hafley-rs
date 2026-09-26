# ryi vs CodeQL, lane Q baseline and lane T2
Implementation tip before this report: `ebdcf4bf`. Sources are frozen `sprefa/v6`; the comparison key is `(source file, enclosing item, target file, target name)`. CodeQL 2.27.1 ran with `--ram=2048 --threads=4`; ryi ran with `RYI_MAX_MEM_MB=2048`. The script used `RYI_CODEQL_REUSE=1` and one database per corpus.

| Corpus | Type agree / ryi-only / CodeQL-only | Call agree / ryi-only / CodeQL-only |
| --- | ---: | ---: |
| Root Rust baseline (423) | 7126 / 13 / 1151 | 10568 / 2843 / 3191 |
| sprefa Rust baseline (552) | 1302 / 1142 / 415 | 3617 / 1818 / 605 |
| TypeScript before T2 (1651), original query | 11326 / 18600 / 6350 | 878 / 6241 / 3890 |
| TypeScript after T2 (1651), original query | 16870 / 13473 / 806 | 4348 / 22883 / 420 |
| TypeScript after T2 (1651), fair query | 16870 / 13473 / 806 | 4447 / 22784 / 420 |

The fair JavaScript query adds module-scope calls only when the target has a body. Its 99 extra rows all agree. The original-query row is the comparable before/after baseline. The 1651-file snapshot contains sorted `rg --files -g '*.ts'` files plus package and tsconfig files; the script's default staged walk contains 1270 TS files.

| Remaining CodeQL-only pattern on 1651 files | Type | Call | Sample |
| --- | ---: | ---: | --- |
| Same-file reference/call | 358 | 153 | `Page`; `sqlShape` |
| Direct relative named type import | 427 | 0 | `IGraphNs` |
| Package/tsconfig-path type import | 15 | 0 | `SqlStatement` |
| Cross-file type with no visible import | 6 | 0 | `Fact` |
| Typed receiver/member call | 0 | 263 | `execute` |
| Direct named call import | 0 | 4 | `buildRuleGraph` |
| Default import, namespace import, re-export chain, `export *`, index resolution | 0 | 0 | none in remaining rows |

Ryi-only call structural buckets: same-file synthetic owner 6237, same-file named owner 5290, cross-file member 9560, cross-file imported spelling 1691, dynamic destructured import 6. [Thirty sampled rows](2026-09-26-ryi-vs-codeql.T2-CALL-AUDIT.tsv) each carry a verdict; 30 correct-but-CodeQL-misses, 0 wrong targets in that sample. The first staged T2 run raised ryi-only calls from 4065 to 10842; the unresolved-import guard removed 1637 of those rows and moved 13 agreements to CodeQL-only. The shadowed-parameter fix removed 27 full-corpus false value refs.
Wrong generated peer checks: `IBindPlanData` has 0 ryi-only target rows; `applyArrivals` has 0 cross-file ryi-only targets; byte-identical source/target peers have 0 cross-file ryi-only rows. Test 181 adds private twins `_5/_6`, exported twins `_7/_8`, an unresolved import, a shadowed parameter, byte-identical files with distinct relative imports, and a named-import static call. The earlier `_5/_6` export removal was for the private-local case; `_7/_8` restores the exported case. Existing expected rows did not change.
Memory: a direct-root run traversed 11605 files, including JSON. At `read_inputs_streamed` fallback-data projection, a 867860-byte `fixture.json` produced 241958 CST nodes; the next 4325384-byte allocation aborted under the 2048 MiB heap cap. Peak sampled RSS was 1581056 KiB. Retained `ProjectInput.output` graphs plus the next fallback CST are the inferred growing structures. The isolated 1651 TS files completed under the cap (sampled peak 666016 KiB).
Perf: sorted first 2000 TypeScript-5.9 compiler `*.ts`, final binary, four extract workers: streamed `ryi fast` 0.53 s wall; SQLite output 3.74 s wall. The SQLite run exceeds the 1.21 s limit derived from the 1.1 s reference.
Gate: `cd crates/sprefa-extract && CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 RYI_MAX_MEM_MB=2048 cargo test --features cli` stopped at `178_ryi_help` after the shared target binary hash changed. Focused 111 (4/4), 178 (2/2), 65 (13/13), 90 (14/14), 136 (3/3), and 181 (1/1) passed. Last five gate lines:
```text
    generated_clap_help_matches_captured_main

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s

error: test failed, to rerun pass `--test 178_ryi_help`
```
