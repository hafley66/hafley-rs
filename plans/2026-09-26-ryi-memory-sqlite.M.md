# ryi fast memory and SQLite, lane M

Code tip: `40ba32b8fc9aaa25274849b53d4a4e443e77e219`.

Fast now skips data inputs over 524,288 bytes before fallback parsing, emits `size_skip`, drops CST/data/flow planes after raw rows reach the sink, and reads mixed roots in eight-file chunks. The new SQLite test passes with a valid 4,800,001-byte JSON file, a TypeScript file, and `RYI_MAX_MEM_MB=256`.

| Memory probe | Result | Peak RSS |
| --- | --- | ---: |
| Before, T2 direct root, 2048 MiB heap cap | Aborted after an 867,860-byte JSON produced 241,958 CST nodes; next 4,325,384-byte allocation failed | 1,581,056 KiB sampled |
| After, direct root `ryi fast ... --sqlite`, same cap | Completed 20,619,459 rows in 59.68 s | 1,258,979,328 bytes, `/usr/bin/time -l` |
| After, streamed sink over 11,229 `rg --files` paths | Completed 20,333,242 rows in 10.08 s | 1,192,755,200 bytes, `/usr/bin/time -l` |

The direct-root SQLite staging file reached 5.52 GB during measurement and was deleted immediately. No temporary database or profile remains. Call/type output graphs remain resident until project resolution; the strict corpus-independent heap bound is still unproved.

Samply profiled the sorted first 2000 TypeScript-5.9 `tests/cases/compiler/*.ts` files on main before changes and on the final writer. Phase timers report bind work including queue waits, writer insert work, and close wall time; bind and insert overlap. The generated schema has zero secondary indexes.

| Phase, seconds | Before | After |
| --- | ---: | ---: |
| Bind | 2.747 | 1.948 |
| Insert | 1.241 | 1.614 |
| Index build | 0.000 | 0.000 |
| Close | 0.021 | 0.021 |
| Total export | 3.148 | 2.404 |
| Samply wall | 4.05 | 3.09 |

Both exports wrote 775,926 fact rows. All 73 tables, including `sqlite_sequence`, have empty `EXCEPT` results in both directions (146 comparisons). `df_lit` remains a table with `_row INTEGER PRIMARY KEY` and no secondary index.

Gate: `cd crates/sprefa-extract && CARGO_TARGET_DIR=$HOME/.cache/boop/cargo-target CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 RYI_MAX_MEM_MB=2048 cargo test --features cli` passed. Last five lines:
```text

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

```
