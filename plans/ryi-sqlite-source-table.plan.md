# ryi SQLite export: one row per file, plain SQL

Measured on `ryii fast --sqlite` over hafley-rxjs `packages` (1624 TS files,
5.39M rows, 928 MB file): `_content_id` 355 MB and `_input_path` 258 MB of
861 MB column bytes, 274x and 301x repeated. `record` repeats the table name
on every row (25.7 MB). The single SQLite writer thread is saturated
(~0.95 core) and is the run's ceiling.

## Type signatures

```tsp
// schema/1_facts.tsp
model Source { record: "source"; @Entity.pk id: int64; path: string; content_id: string; }
model ExportRow { @Entity.pk _row: int64; _source: int64 | null; }
```

```rust
// bin/ryi/0_sqlite.rs
impl Database {
    /// Interns (path, content_id); writes one `source` row the first time.
    pub fn source(&mut self, path: &str, digest: String) -> Result<()>;
}
// bin/ryi/0a_bind.rs: rows bind `_source: Option<i64>` instead of two strings.
```

## Emitted SQL

- `CREATE TABLE "source" ("id" INTEGER PRIMARY KEY, "path" TEXT NOT NULL, "content_id" TEXT NOT NULL)`.
- Fact tables: `"_row" INTEGER PRIMARY KEY, "_source" INTEGER` then their own columns.
  No `record` column (it is the table name), no `AUTOINCREMENT` (the binder
  assigns `_row`).
- Writes: prepared multi-row `INSERT INTO t (...) VALUES (...),(...)`, no
  virtual table. Standard SQL that Postgres and DuckDB accept as written.

## Instance timelines

- `Database`: one per export run. Owns `sources: HashMap<(path, content_id), i64>`
  for the run; ids count from 1 in first-seen order.
- `source()` call: per input file, before that file's rows. A repeat pair
  reuses its id; a new pair writes one `source` row.
- Rows after `clear_source()` (project-wide resolved rows) carry `_source NULL`.

## Storage, reads and writes

- `source` written once per distinct (path, content_id); uniqueness is that pair.
- Readers that filtered or printed `_input_path` / `_content_id` join
  `source ON source.id = t._source`. Views in `0_sqlite.rs` and
  `0d_graph_flow.rs` do the same.
