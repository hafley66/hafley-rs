# ryi fast Rust vs CodeQL, lane A2

Comparison key: `(src file, enclosing item, dst file, dst name)`; macro expansions excluded. `R` means ryi-only and `C` means CodeQL-only. All current runs use `RYI_MAX_MEM_MB=2048`, `CARGO_BUILD_JOBS=4`, `RUST_TEST_THREADS=4`, CodeQL `--ram=2048 --threads=4`, and `RYI_CODEQL_REUSE=1`. The original lane Q measurements below used a larger CodeQL heap.

| Corpus | Q baseline type agree/R/C | A2 before type agree/R/C | A2 step 1 type agree/R/C | Q baseline call agree/R/C | A2 before call agree/R/C | A2 step 1 call agree/R/C |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Root Rust | 7126/13/1151 | 7178/15/1138 | 7178/15/1138 | 10568/2843/3191 | OOM at 2048 MiB | OOM at 2048 MiB |
| sprefa Rust | 1302/1142/415 | 1476/886/361 | 1477/876/360 | 3617/1818/605 | 3728/1908/614 | 3728/1848/614 |

The A2 root CodeQL call query exhausted its 536 MiB Java heap inside the required 2048 MiB total limit, including a source-filtered query. The Q root call row is historical, not a current A2 comparison. Root type and sprefa comparisons use frozen source snapshots and lane binaries. Sprefa CodeQL database: 304743 KiB; initial build 134 s, queries 110 s.

| CodeQL-only source pattern after step 1 | Count | Sample |
| --- | ---: | --- |
| sprefa generated writer type references in one file | 148 | `7_writers_auto.rs`: `Arg -> Arg` |
| sprefa test call sites, mainly local `Probe` methods | 516 | `101_ts_semantic_tsi.rs`: `read -> rows_of` |
| sprefa edit code type references | 52 | `src/edit/_0_seams.rs`: `Key` |
| sprefa edit code call sites | 57 | `src/edit/rust_rename.rs`: `run` |
| root type references, still unpartitioned | 1138 | `TurnEvent` |
| root call sites, Q historical only | 3191 | `default` |

Step 1 removes wrong cross-fixture bindings by limiting module visibility to each fixture, applying that limit to call candidates, keeping prelude `Result` and `Box` external unless locally bound, and excluding generic alias parameters from type-use candidates. Added the `174` scope and `180` call ladder rows. Cross-fixture sprefa wrong targets changed from type 16 to 2 and call 66 to 4; the remaining rows are declared path dependencies. No golden or pin changed.

170 ratchet after step 1: site true positives 748, wrong 1, overbound 2, miss 64; type both 889, fast-only 3, slow-only 2. The 174, 178, 179, 180 tests and full `cargo test --features cli` gate pass. Whole-repo `ryi fast crates/` wall: baseline 28.01 s, step 1 27.94 s (-0.25%).

Gate last five lines:

```text
running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

```
