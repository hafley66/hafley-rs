# Brief: lab-20261003-duckdb-vs-sqlite-post-store

Question: how good is DuckDB as the engine for querying ryi's facts AFTER extraction ("post"),
used like SQLite (plain SQL, native tables), compared with SQLite. No DuckPGQ, no IVM.

## What is known (do not re-measure unless a run below needs it)

- Store lab (`crates/sprefa-extract/bench/labs/lab-20261003-scmpp-sqlite-vs-duckdb/`): same compiled
  scm++ SQL; has-ancestor on all crates SQLite 2.25 s / 31 MB vs DuckDB 0.11 s / 185 MB; rows identical.
- Recursion lab (`.../lab-20261003-recursion-pgq-gritql-sprefa/`): chain links SQLite 1.67 s vs DuckDB
  0.16 s; shortest path plain recursive UNION SQLite 3.23 s vs DuckDB 12.88 s (threads default) /
  3.43 s (threads=1); DuckDB `USING KEY` 0.03 s. SQLite needed ANALYZE; DuckDB needed spans carried.
- Both labs timed the query only, on DuckDB native tables copied from SQLite. Load cost, write
  cost, cold open, concurrency, memory caps, size and build cost are unmeasured.

## Runs (both engines unless stated; every row records rows-equal-to-SQLite yes/no)

| run | what | how |
| --- | --- | --- |
| Q1 query breadth | every scm++ case of the stress lab oracle (174 compiled SQL statements) on both engines | compile each case once with ryii, run the SQL text on SQLite and on DuckDB over the same store; per-case wall, rows equal |
| Q2 ryi questions as SQL | the contract relations and the graph questions as SQL over ryi's `--sqlite` output: callers of NAME, everything NAME reaches (recursive), files importing X, the sprefa `_9j` cycle query shape over `node` + `resolved_edge` | write the SQL once (portable), run on both |
| L1 load paths | getting ryi's SQLite output into DuckDB: (a) query the SQLite file in place via `sqlite_scanner` (ATTACH, no copy), (b) copy into native tables, (c) export Parquet then `read_parquet` | wall + peak RSS of the load, then has-ancestor query time on each of (a)(b)(c) |
| W1 write path | DuckDB as the store ryi writes into: a bench-only Rust program in the lab (its own Cargo.toml, `duckdb` crate with `bundled`), Appender API, writing the same node/capture/dict rows ryii writes for one corpus (read them from the SQLite store as input) vs rusqlite batched inserts (256-row statements, as ryii) | rows/s, wall, peak RSS, file size |
| B1 build cost | the `duckdb` crate in that bench program: clean build time (opt-level per repo rule), incremental rebuild time, final binary size vs the same program without duckdb | numbers only; nothing links into ryii |
| C1 cold vs warm | first query after process start vs repeated query in one process; file opened fresh | both engines |
| M1 memory caps | DuckDB `SET memory_limit` at 64/128/256 MB and `threads` 1/4/default on has-ancestor (all crates) and chain links | wall, peak RSS, spills/errors |
| X1 concurrency | one writer process appending while 2 reader processes query (SQLite WAL vs DuckDB single-writer file): does each reader see consistent rows, do readers block, errors | the shape `ryii watch` + sprefa readers would have |
| S1 size | file size of the same data: SQLite (after VACUUM) vs DuckDB native vs Parquet | bytes |

Corpora: small (`crates/hafley_scm/src` + `crates/sprefa-extract/src`) and all of hafley-rs `crates/`
(Rust only). Same machine; record load average at start of each run; repeat timing runs 3x, report median.

## Rules

- Read `.claude/skills/2026-10-03-ryi-sprefa-boundary/SKILL.md`, `.claude/skills/2026-10-03-sqlite-interning/SKILL.md`,
  `crates/sprefa-extract/AGENTS.md`, repo `CLAUDE.md`.
- DuckDB stays out of every shipped binary; W1/B1 use a bench-only program inside the lab directory,
  never added to a workspace that builds ryii. Its Cargo.toml sets `[profile.dev.package."*"] opt-level = 3`
  and `[profile.dev] opt-level = 1`; time release builds.
- Reuse the store lab's and recursion lab's scripts by path; regenerate their gitignored `db/` stores
  with ryii (release, built once) as needed.
- DuckDB CLI: /opt/homebrew/bin/duckdb (1.5.5). Record versions of everything.
- Tables 1NF, full-word columns; a `grid.html` (bench_grid pattern) for the per-case Q1 table.

## Net new files

`crates/sprefa-extract/bench/labs/lab-20261003-duckdb-vs-sqlite-post-store/`:
`README.md`, `0_stores.sh` (regenerate inputs), `1_query_breadth.py` (Q1), `2_questions.sql` +
`2_questions.sh` (Q2), `3_load_paths.sh` (L1), `4_writer/` (Cargo.toml, src/main.rs; W1 + B1),
`5_cold_warm.sh` (C1), `6_memory.sh` (M1), `7_concurrency.py` (X1), `8_size.sh` (S1),
`9_compare.py` -> `results.tsv`, `grid.html`; `db/` gitignored (add `4_writer/target/` too).

## Deliverable

Branch `lab/20261003-duckdb-vs-sqlite-post-store`; commit the lab and a Results section appended to
this brief. Report: per run the comparison table, any row mismatch explained, and a short verdict
per use: (1) one-shot query engine over ryi output, (2) store ryi writes into, (3) concurrent
reader/writer for watch + sprefa. Time box: one pass; a run that cannot be done within ~20 minutes
is recorded with the exact blocker.

## Results (2026-10-04)

Lab: `crates/sprefa-extract/bench/labs/lab-20261003-duckdb-vs-sqlite-post-store/` (README = method, versions; `results.tsv`
and `grid.html` = every row, medians of 3 processes). DuckDB CLI 1.5.5, duckdb crate 1.10506.0, SQLite 3.53.4.
Load average 5.5 to 8.6 throughout (desktop load), recorded per row. Rows equal = sorted multiset equal to SQLite.

### Q1 query breadth (174 stress-lab cases, compiled SQL text on both engines, CTAS temp per process)

| corpus | cases | rows_equal | order_equal | sqlite_median_s | duckdb_median_s | sqlite_sum_s | duckdb_sum_s | sqlite_max_s | duckdb_max_s | sqlite_rss_median_mb | duckdb_rss_median_mb | duckdb_rss_max_mb |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| small | 174 | 174 | 174 | 0.06 | 0.03 | 166.54 | 7.48 | 55.52 | 0.13 | 7 | 73.5 | 365 |
| crates | 45 | 45 | 45 | 0.40 | 0.07 | 238.94 | 4.46 | 170.84 | 0.29 | 8 | 179 | 676 |

- small: DuckDB faster in 107 cases, SQLite faster in 44 (sub-0.1 s cases, DuckDB process start), 23 ties.
- Slowest SQLite cases: `identifier/has-ancestor/function_item/field` 55.5 s small / 170.8 s crates (DuckDB 0.10 s);
  `field_expression/has-ancestor/call_expression/field` 38.8 s small. ryii itself spends the same time there (store
  build 52 s small / 156 s crates), since ryii runs that SQL on SQLite.
- crates stopped at case 46 of 174 on the 20-minute budget. Blocker: each case rebuilds its own ryii store
  (median 7.2 s) and the SQLite `field` has-ancestor cases run 156 s (ryii) + 171 s (query) + 171 s (dump).
- No mismatch in any case.

### Q2 ryi questions as SQL (`2_questions.sql`, one text both engines; SQLite on ryii's store as written, no index)

| question | corpus | rows | rows_equal | sqlite_s | duckdb_s | sqlite_rss_mb | duckdb_rss_mb |
| --- | --- | --- | --- | --- | --- | --- | --- |
| callers of span.rs#node_text | small | 166 | yes | 0.00 | 0.01 | 5 | 31 |
| reaches from lib.rs#build | small | 34 | yes | 0.01 | 0.01 | 8 | 26 |
| importers of span.rs | small | 61 | yes | 0.00 | 0.01 | 4 | 29 |
| _9j cycles | small | 267 | yes | 0.25 | 0.09 | 18 | 99 |
| callers | crates | 166 | yes | 0.00 | 0.01 | 6 | 31 |
| reaches | crates | 34 | yes | 0.03 | 0.01 | 10 | 29 |
| importers | crates | 66 | yes | 0.00 | 0.01 | 5 | 31 |
| _9j cycles | crates | 267 | yes | 0.56 | 0.10 | 24 | 128 |

DuckDB copy of node + resolved_edge + resolved_import: 1.01 s / 213 MB small, 3.27 s / 264 MB crates. The _9j text
(temp tables, `CREATE INDEX`, table named `call`, `||` over integers) ran unchanged on DuckDB.

### L1 load paths (has-ancestor)

| corpus | path | load_s | load_rss_mb | query_s | query_rss_mb | rows_equal |
| --- | --- | --- | --- | --- | --- | --- |
| small | sqlite (reference) | - | - | 0.45 | 17 | reference |
| small | (a) attach sqlite_scanner | 0.06 | 25 | 3.43 | 91 | yes |
| small | (b) copy native | 0.51 | 139 | 0.04 | 94 | yes |
| small | (c) parquet + read_parquet | 0.49 | 128 | 0.10 | 162 | yes |
| crates | sqlite (reference) | - | - | 2.00 | 43 | reference |
| crates | (a) attach | 0.14 | 25 | 11.05 | 148 | yes |
| crates | (b) copy | 1.58 | 240 | 0.08 | 174 | yes |
| crates | (c) parquet | 1.56 | 212 | 0.13 | 178 | yes |

`SELECT count(*)` on an attached WITHOUT ROWID table fails: `Failed to prepare query "SELECT ROWID FROM "scmpp_node"":
no such column: ROWID` (every scm++ table is WITHOUT ROWID); `count(file)` works.

### W1 write path (scm++ store rows: crates 3443602 node + 278036 capture + 78524 dict; small 1030736 + 84770 + 26889)

| corpus | engine | write_s | index_or_checkpoint_s | rows_per_s | process_wall_s | peak_rss_mb | file_bytes | rows_equal |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| small | input only | - | - | - | 0.53 | 212 | - | - |
| small | sqlite (ryii DDL, 256-row INSERTs, + 2 indexes) | 0.80 | 0.31 | 1432018 | 1.31 | 317 | 59572224 | yes |
| small | duckdb Appender, no keys | 0.66 | 0.02 | 1726594 | 1.19 | 415 | 18624512 | yes |
| small | duckdb Appender, primary keys | 1.09 | 0.00 | 1051733 | 1.60 | 427 | 39596032 | yes |
| crates | input only | - | - | - | 1.71 | 418 | - | - |
| crates | sqlite | 2.81 | 1.12 | 1352836 | 4.49 | 797 | 199163904 | yes |
| crates | duckdb no keys | 2.19 | 0.03 | 1733376 | 3.88 | 847 | 57946112 | yes |
| crates | duckdb primary keys | 3.66 | 0.02 | 1037168 | 5.36 | 920 | 119025664 | yes |

write_s includes SQLite's index build and DuckDB's CHECKPOINT. RSS includes the in-memory input (row "input only").

### B1 build cost (`4_writer`, release, KACHE_DISABLED=1, jobs = 4)

| variant | clean_s | incremental_s (median of 3) | binary_bytes |
| --- | --- | --- | --- |
| rusqlite only | 18.5 | 0.6 | 2375536 |
| rusqlite + duckdb bundled | 272.5 | 1.3 | 42029424 |

An earlier clean build with kache enabled and the machine busier took 552.8 s (cargo "Finished ... in 9m 12s").

### C1 cold vs warm (has-ancestor, median of 3 processes, in-process statement time)

| corpus | engine | iteration1_s | iteration2_s | iteration5_s |
| --- | --- | --- | --- | --- |
| small | sqlite | 0.443 | 0.442 | 0.458 |
| small | duckdb | 0.036 | 0.031 | 0.030 |
| crates | sqlite | 2.014 | 2.001 | 1.918 |
| crates | duckdb | 0.087 | 0.072 | 0.071 |

OS page cache warm in all (no `sudo purge`). Process start + open adds about 0.01 s (DuckDB copy_query process wall
0.04 s small vs 0.031 statement).

### M1 memory caps (DuckDB, crates; wall_s / peak_rss_mb; rows equal in every successful cell)

| memory_limit | threads | ancestor_s | ancestor_rss_mb | chain_s | chain_rss_mb |
| --- | --- | --- | --- | --- | --- |
| 64 MB | 1 | 0.23 | 115 | 0.64 | 105 |
| 64 MB | 4 | Out of Memory: failed to pin block of size 256 KiB | - | 0.26 | 110 |
| 64 MB | default (12) | Out of Memory | - | Out of Memory | - |
| 128 MB | 1 | 0.20 | 148 | 0.66 | 120 |
| 128 MB | 4 | 0.10 | 154 | 0.23 | 125 |
| 128 MB | default | 0.08 | 167 | 0.20 | 150 |
| 256 MB | 1 / 4 / default | 0.19 / 0.11 / 0.08 | 154 / 161 / 174 | 0.63 / 0.21 / 0.20 | 124 / 120 / 148 |
| default | 1 / 4 / default | 0.19 / 0.10 / 0.09 | 155 / 159 / 176 | 0.62 / 0.22 / 0.18 | 118 / 120 / 146 |

Only 64 MB / 1 thread spilled (temporary_storage_bytes 2818048 on ancestor). memory_limit bounds the buffer
manager, so RSS sits above it.

### X1 concurrency (writer 200 x 1000-row transactions, 20 ms apart; 2 readers, one CLI process per query)

| engine | writer_wall_s | writer_open_retries | reader_queries | reader_ok | reader_error | inconsistent | reader_latency_median_s | reader_latency_max_s | distinct_counts_seen |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| sqlite WAL | 4.82 | 0 | 1212 | 1212 | 0 | 0 | 0.007 | 0.019 | 201 |
| duckdb, writer holds connection | 4.98 | 0 | 1159 | 8 | 1151 | 0 | 0.008 | 0.021 | 2 |
| duckdb, writer reopens per batch | 8.05 | 230 | 1687 | 329 | 1358 | 0 | 0.008 | 0.019 | 85 |

DuckDB reader error: `IO Error: Could not set lock on file ... Conflicting lock is held`. With the writer holding the
file, readers succeed only before it opens and after it exits (counts 0 and 200000). Readers fail fast (no blocking);
SQLite readers never block and never see a torn batch.

### S1 size (7 scm++ tables: dicts, node, capture)

| format | small_bytes | crates_bytes |
| --- | --- | --- |
| ryii scm++ store as written (all ryii tables, scmpp_row) | 69533696 | 220463104 |
| SQLite, ryii writer (page 65536, 2 indexes) | 59572224 | 199163904 |
| SQLite VACUUM with indexes | 54722560 | 181993472 |
| SQLite VACUUM tables only | 39387136 | 129892352 |
| SQLite VACUUM tables only, page 4096 | 39170048 | 130899968 |
| DuckDB CTAS copy | 17838080 | 55324672 |
| DuckDB Appender, no keys | 18624512 | 57946112 |
| DuckDB Appender, primary keys | 39596032 | 119025664 |
| Parquet (snappy) | 19721115 | 61732754 |

### Verdicts

1. One-shot query engine over ryi output: DuckDB returned identical rows in every case (174 small, 45 crates,
   8 Q2 questions, 3 load paths) and identical order; per-query wall at or below SQLite except sub-0.1 s queries,
   with 5-10x the RSS (median 74 MB small, 179 MB crates; max 676 MB). The pathological SQLite cases (has-ancestor
   with `field`, 56-171 s) run in 0.10 s. Load path: copy into native tables (1.6 s crates) or Parquet (1.6 s); querying
   the SQLite file in place via sqlite_scanner is 5x slower than SQLite itself (11 s vs 2 s) and breaks `count(*)` on
   WITHOUT ROWID tables.
2. Store ryi writes into: Appender without keys writes 1.28x the rows/s of ryii's batched INSERTs (index build
   included) into a file 3.4x smaller; with the same primary keys it is 0.77x the rows/s and 1.7x smaller. Cost:
   clean build 272 s vs 18.5 s, binary 42 MB vs 2.4 MB, +50-125 MB writer RSS; the boundary skill keeps every engine
   crate out of ryi.
3. Concurrent reader/writer (watch + sprefa): DuckDB's single-process file lock rejects every reader process while
   a writer holds the file (1151 of 1159 reads failed); reopening per batch lets 20% of reads through, costs the
   writer 230 open retries and 1.6x wall. SQLite WAL served 1212 of 1212 reads, consistent, max 19 ms. A DuckDB
   reader of a live store needs a snapshot handoff (e.g. Parquet export or file copy per epoch) or one process
   owning the database.
