# ryi fast Rust vs CodeQL, lane A2

Base tip `14342bf2`; first green step `8834c15f`.

Comparison key: `(src file, enclosing item, dst file, dst name)`; macro expansions excluded. `R` means ryi-only and `C` means CodeQL-only. All current runs use `RYI_MAX_MEM_MB=2048`, `CARGO_BUILD_JOBS=4`, `RUST_TEST_THREADS=4`, CodeQL `--ram=2048 --threads=4`, and `RYI_CODEQL_REUSE=1`. The original lane Q measurements below used a larger CodeQL heap.

| Corpus | Q type agree/R/C | A2 before type | Step 1 type | Step 2 type | Step 3 type | Step 4 type | Step 5 type | Q call agree/R/C | A2 before call | Step 1 call | Step 2 call | Steps 3-5 call |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Root Rust | 7126/13/1151 | 7178/15/1138 | 7172/15/1144 | 7178/15/1138 | 7211/4/1105 | 7386/4/930 | 7409/4/907 | 10568/2843/3191 | OOM | OOM | OOM | OOM |
| sprefa Rust | 1302/1142/415 | 1476/886/361 | 1477/876/360 | 1477/876/360 | 1490/871/347 | 1599/878/238 | 1609/890/228 | 3617/1818/605 | 3728/1908/614 | 3728/1848/614 | 4197/1897/145 |

The A2 root CodeQL call query exhausted its 536 MiB Java heap inside the required 2048 MiB total limit, including a source-filtered query. The Q root call row is historical, not a current A2 comparison. Root type and sprefa comparisons use frozen source snapshots and lane binaries. Sprefa CodeQL database: 304743 KiB; initial build 134 s, queries 110 s.

| CodeQL-only pattern | Step 3 | Step 4 | Step 5 | Sample |
| --- | ---: | ---: | ---: | --- |
| sprefa generated writer enum variant field types | 72 | 72 | 72 | `7_writers_auto.rs`: `Arg -> Arg` |
| sprefa generated writer local type annotations | 71 | 0 | 0 | `7_writers_auto.rs`: `insert_all -> Arg` |
| sprefa test call sites, mainly duplicate local method names | 84 | 84 | 84 | `101_ts_semantic_tsi.rs`: `read -> rows_of` |
| sprefa edit code type references | 52 | 32 | 22 | `_0_seams.rs`: `respell -> Respell` |
| sprefa edit code call sites | 22 | 22 | 22 | `rust_rename.rs`: `harvest -> is_member` |
| root local type annotations recovered | 175 | 0 | 0 | `SymbolSeat` |
| root remaining type references | 1105 | 930 | 907 | `TurnEvent` |
| root call sites, Q historical only | unavailable | unavailable | unavailable | `default` |

Step 1 limits module visibility to each fixture, applies that limit to call candidates, keeps prelude `Result` and `Box` external unless locally bound, and excludes generic alias parameters from type-use candidates. It removed six valid `sqlite-ext` fixture-to-parent type edges. Step 2 preserves declared path dependencies and restores those six rows, with a `174` regression assertion. Cross-fixture sprefa wrong targets changed from type 16 to 2 and call 66 to 4; the remaining rows are declared path dependencies. Step 2 also chooses a method in the caller's file among same-named impls after honoring inherent method precedence. New `180` rows cover duplicate local `Probe::rows` and the trait adapter `Device::ping`; its expected table gains exactly three `fs` rows. Step 3 resolves qualified type re-exports, excludes generic parameter self-bindings and external imports that shadow file types, and preserves uncrated binary-to-library imports. Step 4 collects local body type annotations. Step 5 collects required trait method signatures under synthetic owners. New `174` rows cover both. No existing golden or pin worsened.

170 ratchet after step 5: site true positives 748, wrong 1, overbound 2, miss 64; type both 904, fast-only 3, slow-only 2. Full isolated `cargo test --features cli` gate passed. Whole-repo `ryi fast crates/` paired wall: step 2 27.95 s, step 3 28.18 s (+0.8%); step 3 28.09 s, step 4 27.89 s (-0.7%); step 5 pending (shared Cargo lock).

Gate last five lines:

```text
running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

```
