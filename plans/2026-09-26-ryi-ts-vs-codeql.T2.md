# ryi vs CodeQL, lane T2
Implementation tip before this report: `3f74e5ed`. Sources are frozen `sprefa/v6`; the comparison key is `(source file, enclosing item, target file, target name)`. CodeQL 2.27.1 ran with `--ram=2048 --threads=4`; ryi ran with `RYI_MAX_MEM_MB=2048`. The script used `RYI_CODEQL_REUSE=1` and one database per corpus. [Lane Q's baseline](2026-09-26-ryi-vs-codeql.REPORT.md) remains at `14342bf2`.

| Corpus | Type agree / ryi-only / CodeQL-only | Call agree / ryi-only / CodeQL-only |
| --- | ---: | ---: |
| Root Rust baseline (423) | 7126 / 13 / 1151 | 10568 / 2843 / 3191 |
| sprefa Rust baseline (552) | 1302 / 1142 / 415 | 3617 / 1818 / 605 |
| TypeScript before T2 (1651), original query | 11326 / 18600 / 6350 | 878 / 6241 / 3890 |
| TypeScript after T2 (1651), original query | 16870 / 13473 / 806 | 4352 / 22885 / 416 |
| TypeScript after T2 (1651), fair query | 16870 / 13473 / 806 | 4451 / 22786 / 416 |

The fair JavaScript query adds module-scope calls only when the target has a body. Its 99 extra rows all agree. The original-query row is the comparable before/after baseline. The 1651-file snapshot contains sorted `rg --files -g '*.ts'` files plus package and tsconfig files; the script's default staged walk contains 1270 TS files.

| Remaining CodeQL-only pattern on 1651 files | Type | Call | Sample |
| --- | ---: | ---: | --- |
| Same-file reference/call | 358 | 149 | `Page`; `sqlShape` |
| Direct relative named type import | 427 | 0 | `IGraphNs` |
| Package/tsconfig-path type import | 15 | 0 | `SqlStatement` |
| Cross-file type with no visible import | 6 | 0 | `Fact` |
| Typed receiver/member call | 0 | 263 | `execute` |
| Direct named call import | 0 | 4 | `buildRuleGraph` |
| Default import, namespace import, re-export chain, `export *`, index resolution | 0 | 0 | none in remaining rows |

Ryi-only call structural buckets: same-file synthetic owner 6239, same-file named owner 5290, cross-file member 9560, cross-file imported spelling 1691, dynamic destructured import 6. [Thirty sampled rows](2026-09-26-ryi-vs-codeql.T2-CALL-AUDIT.tsv) each carry a verdict; all 30 remain ryi-only after the final rerun, with 30 correct-but-CodeQL-misses and 0 wrong targets in that sample. The first staged T2 run raised ryi-only calls from 4065 to 10842; the unresolved-import guard removed 1637 of those rows and moved 13 agreements to CodeQL-only. The shadowed-parameter fix removed 27 full-corpus false value refs.
Wrong generated peer checks: `IBindPlanData` has 0 ryi-only target rows; `applyArrivals` has 0 cross-file ryi-only targets; byte-identical source/target peers have 0 cross-file ryi-only rows. Test 181 adds private twins `_5/_6`, exported twins `_7/_8`, an unresolved import, a shadowed parameter, byte-identical files with distinct relative imports, and a named-import static call. The earlier `_5/_6` export removal was for the private-local case; `_7/_8` restores the exported case. New `_15/_16` and `_17/_18` rungs have different declaration spans and opposite export visibility in both directions; their `-s` rows require fast to leave the local call unbound. `_19/_20` has different spans and the same private visibility; its `fs` row preserves local recall after `0aa77e6c`. These distinguish the final rule from the original blanket span rule in `ca0d7ccb`. Existing expected rows did not change; three rows were added.
Memory: the earlier abort came from a direct-root walk of 11605 files, including JSON; it was not the 1651-file TS comparison. At `read_inputs_streamed` fallback-data projection, a 867860-byte `fixture.json` produced 241958 CST nodes; the next 4325384-byte allocation aborted under the 2048 MiB heap cap. Peak sampled RSS was 1581056 KiB. Retained `ProjectInput.output` graphs plus the next fallback CST are the inferred growing structures. The 1651-file run uses a TS-only staged snapshot with package/tsconfig files and `RYI_CODEQL_STAGE=0`; it excludes that JSON and completed under the same cap (sampled peak 666016 KiB). Direct-root fallback-data memory remains unresolved.
The mixed-visibility ambiguity check now compares named function parameter counts when both declarations supply them. It restores `module_plane/shadow_private.ts:parse -> isIdentifier` (0 parameters locally, 1 in the exported peer), moving four corpus calls into agreement without changing type counts. The mutation battery's `step` duplicate (0 versus 0) still has no fast edge.
Perf: sorted first 2000 TypeScript-5.9 compiler `*.ts`, final binary, four extract workers: streamed `ryi fast` 0.32 s wall; SQLite output 3.97 s wall. Streamed extraction meets the 1.21 s limit derived from the 1.1 s reference; SQLite output exceeds it.
Gate: `cd crates/sprefa-extract && CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 RYI_MAX_MEM_MB=2048 cargo test --features cli` passed. Focused 162 (2/2), 90 (14/14), and 181 (1/1) also passed. Last five gate lines:
```text

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
