# scm++ backend B: flat `.scm` plus SQL, run by ryi (plan, 2026-10-02)

Supersedes the in-memory evaluator of `2026-10-02-scmpp-correlated-subqueries.md` (surface and correlation rules stay).
The stopped naive evaluator is parked on branch `wip/scmpp-naive-eval` (6f3afc5a).

scm++ compiles one nested query into N flat tree-sitter patterns plus one SQL statement.
ryi runs each flat pattern as a plain tree-sitter query, writes `capture` rows and CST `node`/`edge` rows,
then runs the SQL. Without `--sqlite` the database is `:memory:` for the run.

## Type signatures

```rust
// crates/hafley_scm/src/scmpp/  (new numbered files)

/// Output of the front end: plain tree-sitter text per level, the join plan, its SQL.
pub struct Compiled {
    pub patterns: Vec<FlatPattern>,
    pub plan: Level,          // level 0 = the outer pattern
    pub sql: String,
}

pub struct FlatPattern {
    pub id: u16,
    pub text: String,         // plain tree-sitter query, root captured as @__root
    pub captures: Vec<Box<str>>,
}

pub struct Level {
    pub pattern: u16,
    pub rels: Vec<Rel>,
    pub conds: Vec<Cond>,     // text predicates that cross levels
}

pub struct Rel {
    pub from: CapRef,         // capture the relation starts at
    pub walk: Walk,           // Parent | Ancestor | Descendant | Precedes | Follows | NthChild(u32)
    pub neighbor: bool,       // stopBy: neighbor
    pub stop: Option<Box<Level>>,  // stopBy: (pattern)
    pub field: Option<Box<str>>,
    pub rows: Rows,           // First (EXISTS) | Each (JOIN)
    pub negated: bool,        // not- prefix
    pub target: Box<Level>,   // nested pattern, may hold its own rels
}

pub struct CapRef { pub level: u8, pub name: Box<str> }
pub enum Cond { TextEq(CapRef, CapRef), TextMatch(CapRef, Box<str>) }

pub fn compile(lang: &tree_sitter::Language, text: &str) -> Result<Compiled, ScmppError>;
fn lower(plan: &Level, patterns: &[FlatPattern]) -> String;    // Level -> SQL
```

```rust
// crates/sprefa-extract/src/0_query.rs: `ryii query --scmpp QUERY.scm [--sqlite DB] PATHS`
fn run_scmpp(cli: QueryArgs, compiled: Compiled, out: &mut Output) -> Result<(), String>;
```

## Front end (compile), once per query

1. Paren pre-pass: one walk with a counter that skips string literals and `;` comments.
   Inside each `(#...?` predicate, each balanced `( ... )` argument is cut out as a nested level.
2. Each level's text, with its predicates removed, becomes a `FlatPattern`; its root gets `@__root`.
   Built-in text predicates (`#eq?`, `#match?`) whose captures all belong to that level stay in its text.
   A text predicate naming a capture from an enclosing level becomes a `Cond`.
3. Each relation predicate becomes a `Rel` whose `target` is the nested level (recursion, any depth).
4. `Query::new` validates each flat pattern once. Kinds and fields are checked against the grammar there.
5. `lower` writes the SQL.

## Lowering rules

| scm++ | SQL |
| --- | --- |
| level L match | `capture` rows with `pattern = L.id` grouped by `match` |
| capture `@x` of level L | `capture` row `name = 'x'` in that match |
| node identity | `(_content_id, start, end, kind)` |
| `has-parent` | `edge` (family cst) with `to = target root`, `from = from` |
| `has-ancestor` | `WITH RECURSIVE anc` up `edge`; `stopBy: (p)` adds `NOT EXISTS` stop match at the step node |
| `has` | the same CTE downwards |
| `stopBy: neighbor` | one `edge` step, no recursion |
| `precedes` / `follows` | same parent, `named_index` greater / smaller; `neighbor` = difference 1 |
| `nth-child N [of L]` | `named_index` among siblings matching L (window `ROW_NUMBER`) |
| `field: f` | `edge.field = 'f'` on the step adjacent to the target (ast-grep meaning) |
| `rows: first` | `EXISTS (...)`, inner captures not selected |
| `rows: each` | `JOIN`, inner captures selected |
| `not-` | `NOT EXISTS (...)`, binds nothing |
| outer capture used inside (`Cond::TextEq`) | `inner.text = outer.text` |
| same name inside and outside | identity join on the node key |

Result: one SELECT whose columns are every exported capture as `name__start`, `name__end`, `name__text`.

## Instance timelines

| instance | born | dies |
| --- | --- | --- |
| `Compiled` | `compile`, once per query | end of the run |
| tree-sitter `Query` per flat pattern | before the file loop | end of the run |
| parse tree per file | file loop | after its captures and CST rows are written |
| SQLite connection | `Output::with_writer` (file or `:memory:`) | `output.finish()` |

## Storage, reads and writes

Writes, per file, in the existing `Output` transaction:
1. `capture` rows for every flat pattern: `pattern` (new column), `match` (new column: per-file ordinal),
   `capture`, `text`, `start`, `end`.
2. CST `node` / `edge` rows (family `cst`) for that file, with new columns on `edge`:
   `field` (nullable), `index` (child position), `named_index` (position among named siblings, null if unnamed),
   and on `node`: `named`.
3. After all files: run `Compiled.sql`; print the rows as JSONL and, with `--sqlite`, store them as `scmpp_row`.

Uniqueness: node key `(_content_id, span__start, span__end, kind)`; match key `(_content_id, pattern, match)`.
Two nodes with the same span and kind (a wrapper whose only child has its span) collapse; the CST rows
carry `kind`, so only same-kind wrappers collide. Recorded as a known limit.

Schema changes go through `crates/sprefa-extract/schema/1_facts.tsp` and regenerate `4_facts.sql`,
`5_facts.json`, `7_writers_auto.rs` with the existing generator.

## Tests

- `hafley_scm`: compile snapshots (flat pattern text + SQL) per lowering rule, inline.
- `sprefa-extract`: `ryii query --scmpp` over fixtures, one row per rule, rows snapshotted;
  the recursion example; correlated `#eq?`; identity join; 2- and 3-level nesting; `rows: each` fan-out;
  `not-` binding nothing; `field:` on has and inside where the field changes the answer; nth-child of a
  supertype kind.
- Growth (hafley-observe `assert_growth_sized`): compile count Constant in query length per level;
  capture/CST row writes Linear in file size.

## Open

- CST rows for every queried file vs only files with at least one level-0 match (smaller DB, two passes).
