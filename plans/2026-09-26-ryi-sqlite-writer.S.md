# ryi SQLite writer, lane S

Base `96292970`. The sorted first 2,000 TypeScript-5.9 compiler `*.ts` files emit 775,926 rows. `RYI_MAX_MEM_MB=2048`, four extract workers, four Cargo jobs, and one benchmark process at a time. Samply recorded both writers; table timers separate bind from insert, which overlap.

| Bulk candidate | Measured workload | Insert rows/s | Result |
| --- | --- | ---: | --- |
| rusqlite prepared 64-row `VALUES` | Full TS, 4 KiB pages | 538,838 | Baseline |
| SQLite `json_each` batch | 50,000 `edge` rows, memory DB | 165,209 | JSON encoding included |
| SQLite `.import` CSV path | 50,000 `edge` rows, memory DB | 196,974 | Does not preserve NULL as NULL |
| rusqlite virtual table, one `INSERT SELECT` per table batch | Full TS, 64 KiB pages | 692,174 | Selected |

`carray` is unavailable in the installed SQLite CLI; `json_each` is the measured SQL-array candidate. The virtual table reads the existing columnar batch directly through rusqlite's `vtab` feature. The `df_lit` table keeps `_row INTEGER PRIMARY KEY` and zero secondary indexes.

| Table | Rows | Bind before / after, s | Insert before / after, s |
| --- | ---: | ---: | ---: |
| `edge` | 321,223 | 0.847 / 0.829 | 0.521 / 0.431 |
| `node` | 341,518 | 0.650 / 0.641 | 0.496 / 0.453 |
| `df_lit` | 25,704 | 0.082 / 0.067 | 0.306 / 0.113 |
| All tables | 775,926 | 1.763 / 1.743 | 1.440 / 1.121 |

| Phase, seconds | Before | After |
| --- | ---: | ---: |
| Index build | 0.000 | 0.000 |
| Close | 0.022 | 0.016 |
| Export total | 2.283 | 2.239 |
| Process wall | 2.716 | 2.473 |

Every one of 73 tables, including `sqlite_sequence`, has zero rows in `EXCEPT` in both directions between baseline and selected writer (146 comparisons). No output rows changed; no ryi-only growth sample applies. The frozen ratchet fixture also produced identical fast and slow database tables under both writer paths, with type-edge counts `(both 904, fast-only 3, slow-only 2)`.

Targets missed: TS SQLite wall 2.473 s versus requested 0.65 s; direct `sprefa/v6` root 84.611 s versus requested 30 s (20,619,867 rows, 4.56 GB database, 699,285,504-byte peak RSS). The root database was deleted immediately.

Gate: `cd crates/sprefa-extract && CARGO_TARGET_DIR=$HOME/.cache/boop/cargo-target CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 RYI_MAX_MEM_MB=2048 RYI_CODEQL_REUSE=1 cargo test --features cli` exited 0. A second lane overwrote the shared `debug/ryi` during the run; the copied lane binary supplied the 146 equality comparisons. The first run's transient fast-only count of 49 returned to the pinned 3 in both writer variants on the frozen fixture. No pin or expected table changed. Last five lines:
```text

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

```
