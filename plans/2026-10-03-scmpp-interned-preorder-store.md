# scm++ store: interned ids, preorder intervals

Rule (.claude/skills/2026-10-03-sqlite-interning): every string interned once to an
integer surrogate id; rows and indexes hold ids only; never copy a string into SQL.

Measured before (216 Rust files, 3.7 MB, `#has-ancestor? @call function_item`):
7.65 s wall, 1.92 GB peak RSS. Top of stack: sqlite3VdbeRecordCompareWithSkip 684,
sqlite3VdbeExec 553, memmove 407, sqlite3BtreeIndexMoveto 329, memcmp 213.
Whole hafley-rs crates/: 24.7 s, 4.48 GB.

## Type sigs

```rust
// ryi bin, scm++ store (replaces capture/edge writes in 2_scmpp.rs)
struct Interner { ids: HashMap<Box<str>, i64> }            // one per run, per dictionary
fn intern(db: &Tx, dict: Dict, text: &str) -> i64            // insert-once, cached id

enum Dict { Path, Kind, Field, CaptureName, Text }

struct NodeRow   { file: i64, pre: i64, last: i64, parent: i64 /* pre of parent, -1 root */,
                   depth: i32, sib: i32 /* named_index */, idx: i32 /* index */,
                   kind: i64, field: i64 /* 0 = none */, start: i64, end: i64, named: bool }
struct CaptureRow { file: i64, pattern: i32, r#match: i64, capture: i64, node: i64 /* pre */ }

fn write_file(tx, interner, compiled, path, tree, src)       // nodes (if plan reads tree) + captures
fn lower(plan, patterns) -> String                           // _3_lower: joins on ids, ranges
fn run_sql(db, compiled) -> rows                             // joins dictionaries only in the final SELECT
```

## Lifetimes

- Interner: one per `ryii query --scmpp` run; dictionaries live in the run's DB.
- File: `file` id = interned path; nodes numbered in preorder per file during one
  tree walk (`pre` counter, `last` = pre of last descendant, set on exit).
- Captures: written per file after its nodes; `node` = `pre` of the captured node,
  found by (start, end, kind) lookup in the walk's own map, not by SQL.
- Compiled SQL: per run; references only integer columns and dictionary ids; strings
  appear only in the final projection via `JOIN dict_text`.

## Storage, reads, writes, uniqueness

| table | columns | primary key / unique | indexes |
| --- | --- | --- | --- |
| dict_path, dict_kind, dict_field, dict_capture, dict_text | id INTEGER PK, text TEXT | UNIQUE(text) (the only string index: the dictionary's own) | — |
| node | file, pre, last, parent, depth, sib, idx, kind, field, start, end, named | PK(file, pre) WITHOUT ROWID | (file, kind, pre), (file, parent, sib) |
| capture | file, pattern, match, capture, node | PK(file, pattern, match, capture, node) WITHOUT ROWID | (pattern, capture, file, node) |
| text_of | file, pre, text | PK(file, pre); text id from dict_text | — (only for captures whose text a predicate or output reads) |

Writes per file: nodes in preorder (append, sorted by PK), captures, text ids for
captured nodes. Reads (lowering):

| relation | SQL shape |
| --- | --- |
| has-ancestor A of N | `a.file = n.file AND a.pre < n.pre AND n.pre <= a.last AND a.kind = :k` |
| has (descendant) D of N | `d.file = n.file AND d.pre > n.pre AND d.pre <= n.last` |
| has-parent | `a.file = n.file AND a.pre = n.parent` |
| precedes / follows / nth-child | same `(file, parent)`, compare `sib`; neighbor = difference 1 |
| stopBy S | `NOT EXISTS` a node S with range strictly between the pair |
| field | `n.field = :field_id` on the step next to the target |
| #eq? cross-level | `t1.text = t2.text` on dict_text ids (equal strings, equal id) |
| #match? / #contains? | `regexp()` / `instr()` over `dict_text.text` joined once per distinct id |

No recursive CTE. Every key is integer.

## Phases

1. scm++ store only (this plan): node/capture/dict tables in the `--scmpp` run DB;
   `_3_lower` emits id/range SQL; snapshots in hafley_scm/tests/_1_scmpp_compile.rs
   regenerate; oracle (174 cases) and doc examples must stay identical; measure the
   two queries above before/after.
2. ryi fact tables (schema/1_facts.tsp: `_content_id`, `kind`, `path`, `name`, ... as
   strings): same rule through the TypeSpec SQL emitter's interned dictionaries;
   separate plan.

## Engine neutrality (approved 2026-10-03)

The interned-id + preorder-range design is engine independent. Phase 1 keeps it so:

1. `_3_lower` emits plain SQL: joins, range predicates, `NOT EXISTS`, one `regexp`
   scalar function. No SQLite-only syntax (no `WITHOUT ROWID` inside the query, no
   recursive CTE).
2. Row writing sits behind one seam in the ryi bin, so rows can go to SQLite or to
   Arrow/Parquet.
3. Bench: the same compiled SQL on SQLite and DuckDB (`duckdb` crate, dev/bench only,
   not in the shipped binary) for the two profiled queries over hafley-rs crates/;
   table of wall, peak RSS, DB size per engine. The numbers decide the engine.

Time box: phase 1 is one writer pass; stop and report if it is not converging.

## Results (phase 1, 2026-10-03)

Implemented on `feature/scmpp-store`. Lab: `crates/sprefa-extract/bench/labs/lab-20261003-scmpp-sqlite-vs-duckdb/`
(`1_before_after.sh`, `2_engines.sh`, `ancestor.sql`/`plain.sql` = the compiled SQL, `before_after.tsv`, `engines.tsv`).

### Shape as built

| table | columns | key | indexes |
| --- | --- | --- | --- |
| scmpp_dict_path, scmpp_dict_kind, scmpp_dict_field, scmpp_dict_capture, scmpp_dict_text | id, text | id INTEGER PK; UNIQUE(text) | none |
| scmpp_node | file, pre, last, parent, depth, sib, idx, kind, field, start, end, named | PK(file, pre) WITHOUT ROWID | (file, parent, sib), built after the load |
| scmpp_capture | file, pattern, match, capture, node, start, end, text | PK(file, pattern, match, capture, node) WITHOUT ROWID | (pattern, capture, file, node), built after the load |

Differences from the plan above:

- Table names carry a `scmpp_` prefix: the fact DDL already owns `node`, `edge`, `capture`.
- `text_of` folded into `scmpp_capture.text` (text id; NULL on `@__root` rows), with `start`, `end` on the
  capture row: the relation-free plan writes no node rows and still projects spans.
- No `(file, kind, pre)` index: no lowered SQL reads `kind`.
- Capture and field ids the SQL names come from `Compiled.captures` / `Compiled.fields`
  (`hafley_scm::scmpp::_3_lower::dictionaries`); the store interns those lists first, so entry i has id i+1,
  and errors if a dictionary already holds a name at another id.
- Text predicates: `capture.text IN (SELECT id FROM scmpp_dict_text WHERE regexp(...))` (one evaluation per
  dictionary entry). Projection text and path: correlated scalar subqueries on the dictionary id, so they add no
  join (SQLite's 64-table join limit counts only capture joins, as before). `ORDER BY path` sorts by path text.
- has-ancestor puts its range on the target capture (`r.node < from.node AND from.node <= a.last`): with the
  range on `a.pre`, SQLite 3.53 (bundled and CLI) scanned node ranges, 11.3 s vs 0.5 s on scm_extract_src.
- Seam: trait `Rows { dict, node, capture }` in `src/bin/ryi/1a_scmpp_rows.rs` (store, interners, one
  `walk_streaming` preorder pass per file, `last` by one reverse pass); SQLite writer `1b_scmpp_sqlite.rs`
  (256-row multi-row INSERTs). Write-path candidates: rusqlite prepared row-at-a-time; multi-row VALUES (chosen);
  rusqlite `rarray` per column; the TypeSpec `Binder` (fact tables only).

### Correctness

| check | result |
| --- | --- |
| hafley_scm `_1_scmpp_compile` snapshots | 17 regenerated, 18 pass |
| stress oracle (174 cases, crates/hafley_scm/src/lang) | 174 agree, 114 non-empty, 175,706 rows; columns 1-8 identical to oracle.tsv and to a before-binary run |
| doc examples (3_doc_examples.py) | 20 match, 0 mismatch |
| stdout byte compare, before vs after, fx/*.scm x fx/*.rs | 227 identical, 0 differ, 4 before-timeouts (15 s; S4 deep nesting) |
| stdout byte compare, the 4 measured runs | 4 identical |

Oracle total wall: 95.45 s before, 42.14 s after.

### Before / after (release ryii, `--sqlite`, main checkout, `--pattern '*.rs'`)

ancestor = `((call_expression) @call (#has-ancestor? @call function_item))`, plain = `((call_expression) @call)`;
crates = hafley-rs `crates/`; scm_extract_src = `crates/hafley_scm/src` + `crates/sprefa-extract/src`.

| query | corpus | build | wall_s | peak_rss_mb | db_mb | rows |
| --- | --- | --- | --- | --- | --- | --- |
| ancestor | crates | before | 27.18 | 1264 | 2026.1 | 124241 |
| ancestor | crates | after | 7.31 | 354 | 203.4 | 124241 |
| ancestor | scm_extract_src | before | 6.88 | 1055 | 602.5 | 36242 |
| ancestor | scm_extract_src | after | 1.95 | 146 | 61.4 | 36242 |
| plain | crates | before | 2.63 | 422 | 169.7 | 124661 |
| plain | crates | after | 2.38 | 217 | 49.7 | 124661 |
| plain | scm_extract_src | before | 0.73 | 166 | 53.5 | 36410 |
| plain | scm_extract_src | after | 0.63 | 92 | 18.2 | 36410 |

ancestor/crates after: 3,255,814 node rows, 262,060 capture rows, 1,176 paths.

### SQLite vs DuckDB, same compiled SQL

Query only (`CREATE TABLE bench_row AS <sql>`), over the after stores. SQLite: CLI 3.53.4 on the ryii DB
(indexes present, no ANALYZE). DuckDB: CLI 1.5.5, tables copied into a native file via the sqlite extension,
default threads (12 cores). Rows byte-identical between engines in all 4 cells.

| engine | query | corpus | wall_s | peak_rss_mb |
| --- | --- | --- | --- | --- |
| sqlite | ancestor | crates | 2.25 | 31 |
| duckdb | ancestor | crates | 0.11 | 185 |
| sqlite | ancestor | scm_extract_src | 0.51 | 12 |
| duckdb | ancestor | scm_extract_src | 0.07 | 95 |
| sqlite | plain | crates | 0.26 | 32 |
| duckdb | plain | crates | 0.11 | 167 |
| sqlite | plain | scm_extract_src | 0.06 | 15 |
| duckdb | plain | scm_extract_src | 0.06 | 82 |
