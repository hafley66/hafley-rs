# ryi: many `.scm` patterns over one root, and a DuckDB/PGQ target

Date: 2026-10-30

## 1. Current cost model of `ryi query`

Path: `crates/sprefa-extract/src/0_query.rs` -> `hafley_scm/src/read/lang/2_source_query.rs:115`.

`QueryArgs` (`ryi-proto/src/gen/ops_auto.rs:295`) takes one `--query: String`.
For `N` patterns over `F` files the user runs `N` processes. Per process, per file:

| step | where | count per run of N patterns |
| --- | --- | --- |
| read bytes | `0_query.rs` `source_bytes` | N x F |
| new `TreeParser` + parse | `2_source_query.rs:120-128` | N x F |
| `hafley_scm::build` (compile query) | `2_source_query.rs:129` | N x F (inside the file loop) |
| `hafley_scm::run` (cursor pass) | `2_source_query.rs:134` | N x F |
| row insert, `"query"` = full query text per capture row | `0_query.rs` `database.insert` | per capture |

Files run sequentially; no rayon over `paths`.

## 2. Speedups, ordered by expected gain

1. `--query` -> `Vec<String>` plus `--scm <file>...` (each file one pattern set).
   Compile each `QueryExt` once per language, outside the file loop.
   Removes `(N x F) - (N x langs)` compiles.
2. Parse each file once; run all `N` compiled queries over the same `Tree`.
   Removes `(N - 1) x F` parses.
3. Merge the `N` sources into one `tree_sitter::Query` (concatenated patterns).
   One cursor pass per file instead of `N`. `row.pattern` (already kept,
   `TreeSitterSpannedMatch.pattern`) maps back to `(scm_file, pattern)` via a
   `pattern_start_byte` / per-file pattern-count offset table.
   Open item: `hafley_scm::build` splits general predicates into kind queries
   (`pipeline/split_predicates_into_kind_queries`); merged query must keep the
   per-pattern predicate lists aligned with the offset table.
4. Parallel files: `paths.par_iter()` producing per-file arenas, one writer
   thread owning the SQLite connection (single writer).
5. Row shape: `query` table `(query_id, scm_path, pattern_index, text)`;
   capture rows carry `query_id` integer instead of repeating the query text.

Step trace, N = 40 scm files, F = 5000 files, 1 language:

```
today      : parses 200000, compiles 200000, cursor passes 200000, processes 40
after 1    : parses 200000, compiles 40,     cursor passes 200000, processes 1
after 1+2  : parses 5000,   compiles 40,     cursor passes 200000
after 1+2+3: parses 5000,   compiles 1,      cursor passes 5000
after 4    : same counts, wall time / cores
```

## 3. Engine status (searched 2026-10-30)

| engine | SQL/PGQ | IVM | status |
| --- | --- | --- | --- |
| SQLite | none; no announcement or proposal found | own `sqlite-ext` crate / `sqlite-ivm-vtab-lifecycle` skill | recursive CTE only |
| PostgreSQL 19 | committed 2026-03-16, reverted before release (beta 4 notes "Revert SQL/PGQ") | pg_ivm | earliest return PG 20 (one source) |
| DuckDB + duckpgq | community ext; docs say use DuckDB 1.4.4, not 1.5.x; PR #325 (merged 2026-10-13) moves grammar to PEG ext targeting DuckDB 2.0, opt-in via `SET` | n/a | version-split |
| DuckDB + openivm | n/a | source build against DuckDB 1.5.4; community-extension release targeted end 2026; issue #92: loading sets `preserve_insertion_order=false` db-wide | experimental |

Version conflict: duckpgq pins 1.4.4 or 2.0, openivm pins 1.5.4. No single
DuckDB build loads both as of the sources found.

## 4. Dialect delta: ryi SQL today vs DuckDB

From `crates/sprefa-extract/src/bin/ryi/0_sqlite.rs`:

| construct | site | SQLite | DuckDB | portable form |
| --- | --- | --- | --- | --- |
| `e."caller_name" IS w."dst_name"` (null-safe eq) | `reach_walk_sql` :113 | yes | `IS` only with NULL/bool | `IS NOT DISTINCT FROM` (SQLite >= 3.39, DuckDB yes) |
| `WITH RECURSIVE ... UNION ... depth < 32` | `reach_walk_sql` | yes | yes; also `USING KEY` (1.3+) | as-is |
| `json_extract(col, '$[0].id')` | `type_evidence` view :145 | yes | `json` ext, same path syntax, returns JSON not text | `->>` operator works in both |
| `CREATE VIEW ...; CREATE VIEW ...;` multi-statement | `graph_views_sql` | yes | yes | as-is |

Reading a ryi SQLite db from DuckDB needs no ryi change:
`INSTALL sqlite; ATTACH 'out.db' (TYPE sqlite);` then
`COPY (SELECT * FROM out.capture) TO 'capture.parquet';`.

## 5. PGQ shape over ryi tables (duckpgq syntax)

```sql
CREATE PROPERTY GRAPH code
  VERTEX TABLES (symbol)
  EDGE TABLES (resolved_edge
    SOURCE KEY (caller_path, caller_name) REFERENCES symbol (path, name)
    DESTINATION KEY (callee_path, callee_name) REFERENCES symbol (path, name)
    LABEL calls);

FROM GRAPH_TABLE (code
  MATCH (a:symbol)-[c:calls]->{1,32}(b:symbol)
  COLUMNS (a.path AS src_path, a.name AS src_name, b.path AS dst_path, b.name AS dst_name));
```

Requires a `symbol` vertex table; today ryi has edge rows only
(`resolved_edge`, `resolved_type_edge`). `reach` view = this MATCH.

## 6. Open questions for the user

- Merged-query (step 3) vs per-query cursor (step 2 only): step 3 needs the
  predicate-split offset work in `hafley_scm`.
- Emit target: keep SQLite as write format and attach from DuckDB, or add a
  direct DuckDB/parquet writer.
- Which DuckDB line to pin: 1.4.4 (duckpgq stable), 1.5.4 (openivm), 2.0 (duckpgq PEG).

## Sources

- https://jamsql.com/blog/2026-07-11-sql-pgq-graph-table-oracle-postgresql/
- https://dev.to/amareswer/postgresql-19-whats-actually-shipping-what-got-cut-4b1j
- https://www.devx.com/cybersecurity/postgresql-19-new-features-release-date/
- https://duckdb.org/docs/current/guides/sql_features/graph_queries
- https://duckdb.org/community_extensions/extensions/duckpgq
- https://github.com/cwida/duckpgq-extension/pull/325
- https://github.com/ila/openivm
- https://github.com/ila/openivm/issues/92
