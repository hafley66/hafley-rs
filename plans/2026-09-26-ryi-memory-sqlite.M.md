# ryi fast memory and SQLite, lane M

Code tip: `3e1a55388cb240bddb22ac56cfa87884c149dfe0`.

Fast skips data inputs over 524,288 bytes before parsing and emits `size_skip`. It drops raw-only planes after emission. Above 4096 inputs, it retains compact definition nodes and module indexes, then reparses eight-file chunks for call, import, type, and SCM rows. The 4,800,001-byte JSON test passes under `RYI_MAX_MEM_MB=256`; a 4097-file mixed-language test matches the original resolved row order.

| Memory probe | Result | Peak RSS |
| --- | --- | ---: |
| Before, T2 direct root, 2048 MiB heap cap | Aborted after an 867,860-byte JSON produced 241,958 CST nodes; next 4,325,384-byte allocation failed | 1,581,056 KiB sampled |
| After raw-plane release, direct root `--sqlite` | Completed 20,619,459 rows in 59.68 s | 1,258,979,328 bytes, `/usr/bin/time -l` |
| After bounded resolve, direct root `--sqlite`, same cap | Completed 20,619,459 rows in 82.23 s | 655,671,296 bytes, `/usr/bin/time -l` |

The final direct-root staging file reached 5.51 GB during measurement and was deleted immediately. No temporary database or profile remains. Retained per-file graphs are capped at 4096 inputs; the larger path keeps definition/module indexes plus one eight-file graph chunk. The remaining index size depends on the number of definitions and imports.

Samply profiled the sorted first 2000 TypeScript-5.9 `tests/cases/compiler/*.ts` files on main before changes and on the final writer. Phase timers report bind work including queue waits, writer insert work, and close wall time; bind and insert overlap. The generated schema has zero secondary indexes.

| Phase, seconds | Before | After |
| --- | ---: | ---: |
| Bind | 2.747 | 1.781 |
| Insert | 1.241 | 1.482 |
| Index build | 0.000 | 0.000 |
| Close | 0.021 | 0.022 |
| Total export | 3.148 | 2.317 |
| Samply wall | 4.05 | 3.09 |

Both exports wrote 775,926 fact rows. Final versus original main: all 73 tables, including `sqlite_sequence`, have empty `EXCEPT` results in both directions (146 comparisons). `df_lit` remains a table with `_row INTEGER PRIMARY KEY` and no secondary index.

Gate: `cd crates/sprefa-extract && CARGO_TARGET_DIR=$HOME/.cache/boop/cargo-target CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 RYI_MAX_MEM_MB=2048 cargo test --features cli` passed. The first run failed the wall-time ratio in `46_resolve_scaling` (0.120/0.042 s); focused and full reruns passed with no test changes. Last five lines:
```text

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

```
