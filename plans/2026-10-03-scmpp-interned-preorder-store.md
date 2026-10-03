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
