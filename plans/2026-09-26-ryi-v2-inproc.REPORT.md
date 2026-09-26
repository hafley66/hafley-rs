# ryi V2: in-process operations and SQLite cache

Code tip `6cf1ee84`; baseline `f5fc2820`. Commits: `41b2fcb6`, `cd8ea5f7`, `6cf1ee84`.

## Transport

`ops.rs` invokes Rust handlers in-process. The extraction verbs write to an injected sink; remaining stdout-based verbs are relayed through a bounded channel. `RYI_BIN` and child `ryi` launches are gone. Test 178 retains its 14-verb CLI/Router parity table and adds a no-child row: the proof server has `RYI_BIN` unset, no `ryi` on `PATH`, and its launched executable removed. Invalid CLI-style exits stay inside the operation.

## SQLite measurements

Two runs per cell, wall seconds / peak RSS MiB. TS is the sorted first 2,000 `*.ts` under `TypeScript-5.9/tests/cases/compiler`; `crates/` is the worktree directory. `RYI_MAX_MEM_MB=2048`, `CARGO_BUILD_JOBS=4`, `RUST_TEST_THREADS=4`, one run at a time.

| Corpus | Parent: 64 KiB + sync | 64 KiB, no sync | Final: 4 KiB, no sync |
| --- | --- | --- | --- |
| TS | 1.53/414; 2.61/382 | 1.53/379; 1.80/361 | 2.12/380; 1.48/370 |
| `crates/` | 12.00/452; 11.36/444 | 12.19/449; 11.38/442 | 11.64/445; 12.27/448 |

Parent and no-sync differ only by removal of `sync_all`; these two runs do not show a consistent wall reduction. Lane T previously measured 571 ms in that call. The database is a derived cache published from a private staging file; `journal_mode=OFF`, `synchronous=OFF`, and one bulk transaction were already in place. `crates/` gained 180 fact rows from source edits between parent and final measurements.

| TS `dbstat` object | Parent bytes | Final bytes |
| --- | ---: | ---: |
| `df_lit` | 194,052,096 | 174,039,040 |
| `node` | 15,335,424 | 15,630,336 |
| `edge` | 12,386,304 | 12,673,024 |
| Database file | 229,834,752 | 206,143,488 |

[All 74 `dbstat` objects, including 72 fact tables](./2026-09-26-ryi-v2-dbstat.tsv). All 72 table row counts match; `df_lit` has zero rows in either direction of `EXCEPT`. No golden, snapshot, pin, or expected table changed. A text-interning view was measured but removed because `tests/0_sqlite.rs` requires `df_lit` to remain a table with `_row INTEGER PRIMARY KEY` and no secondary index.

## Gate

`CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/cargo-target CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 RYI_MAX_MEM_MB=2048 cargo test --features cli` exited 0. Last five lines:

```text

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
