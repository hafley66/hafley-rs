# ryi fast Rust vs CodeQL, lane A2

Parent tip `75dc198d`; base `14342bf2`. Comparison key: `(src file, enclosing item, dst file, dst name)`; macro expansions excluded. `R` is ryi-only, `C` CodeQL-only. Runs use `RYI_MAX_MEM_MB=2048`, `CARGO_BUILD_JOBS=4`, `RUST_TEST_THREADS=4`, CodeQL `--ram=2048 --threads=4`, and `RYI_CODEQL_REUSE=1` on frozen sources. Lane Q used a larger CodeQL heap.

| Type corpus (agree/R/C) | Q | A2 before | S1 | S2 | S3 | S4 | S5 | S6 | S7 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Root Rust | 7126/13/1151 | 7178/15/1138 | 7172/15/1144 | 7178/15/1138 | 7211/4/1105 | 7386/4/930 | 7409/4/907 | 7413/4/903 | 7413/24/903 |
| sprefa Rust | 1302/1142/415 | 1476/886/361 | 1477/876/360 | 1477/876/360 | 1490/871/347 | 1599/878/238 | 1609/890/228 | 1622/891/215 | 1693/891/144 |

| Call corpus (agree/R/C) | Q | A2 before | S1 | S2-S7 |
| --- | ---: | ---: | ---: | ---: |
| Root Rust | 10568/2843/3191 | OOM | OOM | OOM |
| sprefa Rust | 3617/1818/605 | 3728/1908/614 | 3728/1848/614 | 4197/1897/145 |

Root CodeQL call query exhausted its 536 MiB Java heap inside the required 2048 MiB total limit, including a source-filtered query. Its Q row is historical. Sprefa CodeQL database: 304743 KiB; initial build 134 s, queries 110 s.

| CodeQL-only pattern | Before fix | After S7 | Sample |
| --- | ---: | ---: | --- |
| sprefa generated writer variant field type | 72 | 1 | `7_writers_auto.rs`: `Arg -> Arg` |
| sprefa generated writer local annotation | 71 | 0 | `7_writers_auto.rs`: `insert_all -> Arg` |
| sprefa test duplicate method call | 516 | 84 | `101_ts_semantic_tsi.rs`: `read -> rows_of` |
| sprefa edit type reference | 52 | 22 | `_0_seams.rs`: `respell -> Respell` |
| sprefa edit call | 57 | 22 | `rust_rename.rs`: `harvest -> is_member` |
| root local type annotation | 175 | 0 | `SymbolSeat` |
| root remaining type references | 1105 | 903 | `TurnEvent` |
| root call sites, Q only | 3191 | unavailable | `default` |

S1 limits fixture visibility and prelude fallbacks. S2 restores six declared `sqlite-ext` path-dependency edges and resolves local impl methods. S3 resolves qualified type re-exports and removes generic-parameter and external-import false bindings. S4 collects local annotations. S5 collects required trait signatures under synthetic owners. S6 collects qualified or parameterized impl associated type assignments. S7 adds variant-owned edges for qualified field types. New `174`/`180` rows cover each; existing goldens and pins are unchanged. The 20 new root ryi-only S7 rows are source-matching variant field types; the four earlier root R rows are macro-expansion exclusions.

170 after S7: call true positives 748, wrong 1, overbound 2, miss 64; type both 904, fast-only 3, slow-only 2. Full isolated `cargo test --features cli` gate passed. Paired whole-repo `ryi fast crates/` wall: S2 27.95 s, S3 28.18 s (+0.8%); S3 28.09 s, S4 27.89 s (-0.7%); S4 27.79 s, S7 28.32 s (+1.9%).

Gate last five lines:

```text
running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

```
