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
