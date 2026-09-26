# ryi fast Rust vs CodeQL, lane A2

Base tip `14342bf2`; first green step `8834c15f`.

Comparison key: `(src file, enclosing item, dst file, dst name)`; macro expansions excluded. `R` means ryi-only and `C` means CodeQL-only. All current runs use `RYI_MAX_MEM_MB=2048`, `CARGO_BUILD_JOBS=4`, `RUST_TEST_THREADS=4`, CodeQL `--ram=2048 --threads=4`, and `RYI_CODEQL_REUSE=1`. The original lane Q measurements below used a larger CodeQL heap.

| Corpus | Q type agree/R/C | A2 before type | A2 step 1 type | A2 step 2 type | Q call agree/R/C | A2 before call | A2 step 1 call | A2 step 2 call |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Root Rust | 7126/13/1151 | 7178/15/1138 | 7172/15/1144 | 7178/15/1138 | 10568/2843/3191 | OOM | OOM | OOM |
| sprefa Rust | 1302/1142/415 | 1476/886/361 | 1477/876/360 | 1477/876/360 | 3617/1818/605 | 3728/1908/614 | 3728/1848/614 | 4197/1897/145 |

The A2 root CodeQL call query exhausted its 536 MiB Java heap inside the required 2048 MiB total limit, including a source-filtered query. The Q root call row is historical, not a current A2 comparison. Root type and sprefa comparisons use frozen source snapshots and lane binaries. Sprefa CodeQL database: 304743 KiB; initial build 134 s, queries 110 s.

| CodeQL-only pattern | Step 1 count | Step 2 count | Sample |
| --- | ---: | ---: | --- |
| sprefa generated writer enum variant field types | 73 | 73 | `7_writers_auto.rs`: `Arg -> Arg` |
| sprefa generated writer local type annotations | 71 | 71 | `7_writers_auto.rs`: `insert_all -> Arg` |
| sprefa test call sites, mainly duplicate local method names | 516 | 84 | `101_ts_semantic_tsi.rs`: `read -> rows_of` |
| sprefa edit code type references | 52 | 52 | `_0_seams.rs`: `respell -> Respell` |
| sprefa edit code call sites | 57 | 22 | `rust_rename.rs`: `harvest -> is_member` |
| root type references, still unpartitioned | 1138 | pending | `TurnEvent` |
| root call sites, Q historical only | 3191 | unavailable | `default` |

Step 1 limits module visibility to each fixture, applies that limit to call candidates, keeps prelude `Result` and `Box` external unless locally bound, and excludes generic alias parameters from type-use candidates. It removed six valid `sqlite-ext` fixture-to-parent type edges. Step 2 preserves declared path dependencies and restores those six rows, with a `174` regression assertion. Cross-fixture sprefa wrong targets changed from type 16 to 2 and call 66 to 4; the remaining rows are declared path dependencies. Step 2 also chooses a method in the caller's file among same-named impls after honoring inherent method precedence. New `180` rows cover duplicate local `Probe::rows` and the trait adapter `Device::ping`; its expected table gains exactly three `fs` rows. No existing golden or pin worsened.

170 ratchet after steps 1 and 2: site true positives 748, wrong 1, overbound 2, miss 64; type both 889, fast-only 3, slow-only 2. Full isolated `cargo test --features cli` gate passed. Whole-repo `ryi fast crates/` paired wall: baseline 28.67 s, step 2 28.33 s (-1.2%).

Gate last five lines:

```text
running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

```
