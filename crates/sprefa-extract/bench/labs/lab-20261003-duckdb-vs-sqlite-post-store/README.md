# lab-20261003-duckdb-vs-sqlite-post-store

Brief: `plans/2026-10-03-duckdb-post-store-lab.brief.md` (Results appended there). DuckDB used like SQLite (plain SQL,
native tables) over ryii's output after extraction, against SQLite. No DuckPGQ, no IVM. No product code changes;
DuckDB enters only the bench program `4_writer/` (own workspace root, never built with ryii).

## Run

```
cd crates/sprefa-extract && RCARGO_OFF=1 cargo build --release --bin ryii --features cli,ts-checker,typespec
LAB=crates/sprefa-extract/bench/labs/lab-20261003-duckdb-vs-sqlite-post-store
$LAB/4_writer/run.sh sql          # Q1 SQL printer (hafley_scm::scmpp::compile, the function ryii calls), target/sql
$LAB/4_writer/run.sh b1           # B1: clean + 3 incremental release builds with and without duckdb (KACHE_DISABLED=1)
$LAB/0_stores.sh                  # db/ancestor-<corpus>.db (scm++ has-ancestor store), db/resolve-<corpus>.db, db/lab-<corpus>.duckdb
$LAB/3_load_paths.sh              # L1 (also writes db/ancestor-<corpus>.duckdb and db/parquet-<corpus>/ used below)
$LAB/2_questions.sh               # Q2
$LAB/5_cold_warm.sh               # C1
$LAB/6_memory.sh crates           # M1
$LAB/4_writer/run.sh w1           # W1
$LAB/8_size.sh                    # S1 (reads W1 and L1 outputs)
python3 $LAB/7_concurrency.py     # X1
BUDGET_S=1800 python3 $LAB/1_query_breadth.py small; BUDGET_S=1200 python3 $LAB/1_query_breadth.py crates   # Q1, resumable
python3 $LAB/9_compare.py         # results.tsv, grid.html
```

`0_measure.py` is the one timing implementation: `/usr/bin/time -l`, REPEAT (3) processes, one row per process in
`db/raw.tsv` with the 1-minute load average at its start; `9_compare.py` reports medians.

## Inputs

| corpus | paths | scmpp_node rows | scmpp_capture rows | has-ancestor rows |
| --- | --- | --- | --- | --- |
| small | `crates/hafley_scm/src`, `crates/sprefa-extract/src` | 1030736 | 84770 | 40467 |
| crates | `crates` (Rust) | 3443602 | 278036 | 131834 |

Worktree base 1051990d. Chain links (M1) run on the recursion lab's `lab-crates.duckdb`, copied into `db/` (35484 rows,
equal to that lab's SQLite R1 output).

## Method notes

- Timed query = `CREATE TEMP TABLE bench_row AS <sql>` in a fresh CLI process (`-readonly`; SQLite `temp_store=MEMORY`).
  Rows equal = a separate untimed dump of both engines (unit/record separators), compared as sorted multisets
  (`rows_equal`) and in emitted order (`order_equal`).
- SQLite runs the store exactly as ryii leaves it: scm++ store with ryii's two post-load indexes, resolve store with
  no index, no `ANALYZE` anywhere. DuckDB tables are created by `CREATE TABLE AS` from `sqlite_scan` (no keys).
- Q1: SQL text from `lab_writer sql` (feature `sql`); identical to the store lab's `ancestor.sql`. A case whose
  first process runs past 10 s is not repeated (`repeats` = 1).
- W1: `lab_writer write` reads all dict/node/capture rows of the scm++ store into memory, then times the write only.
  SQLite = ryii's DDL and pragmas, dict rows one statement each, node/capture 256-row multi-row INSERTs, one transaction,
  then ryii's two indexes (`write_s` includes them). DuckDB = Appender per table, then CHECKPOINT; `duckdb_pk` keeps
  the same primary/unique keys. Peak RSS includes the in-memory input (engine `none` = input only).
- X1: writer `lab_writer append` = 200 transactions of 1000 rows, 20 ms apart; two reader threads each spawn one CLI
  process per query until the writer exits. `duckdb` keeps one connection open; `duckdb_reopen` opens, appends,
  checkpoints and closes per batch, retrying the open every 5 ms.
- C1: OS page cache stays warm (no `sudo purge`); "cold" = first statement after process start and file open.
- M1 memory_limit caps DuckDB's buffer manager only; process RSS includes the CLI and code.
- Machine load: the 1-minute load average was 5.5 to 8.6 through every run (desktop processes); it is in each row.

## Versions

| tool | version |
| --- | --- |
| ryii | worktree base 1051990d, release, features cli,ts-checker,typespec |
| DuckDB CLI | v1.5.5 (d8cdaa33fd), Homebrew; sqlite_scanner and parquet extensions |
| duckdb crate | 1.10506.0 (`bundled`), libduckdb-sys 1.10506.0 |
| rusqlite | 0.40.2 (`bundled`) |
| SQLite CLI | 3.53.4 |
| Python | 3.14.7 |
| rustc / cargo | 1.100.0-nightly (17fd5b8a3 2026-08-28) / e8cb624d5; `~/.cargo/config.toml` jobs = 4 |
