# scm++ compiler only (option D)

Decision 2026-10-02: scm++ is a pure compiler (text in, flat .scm + SQL text out).
ryi owns files, trees, rows, SQLite and the one SQL executor. The build path
(bundled .scm, gate rules) no longer evaluates relation predicates.
Later: sprefa emits the SQL in place of `_3_lower` (not in this change).

## Zoom 1

```rust
// 1. scm++ = pure compiler, hafley_scm::scmpp
fn compile(lang, text) -> Compiled            // _1_parens -> _2_compile -> _3_lower
struct Compiled { patterns: Vec<FlatPattern>, plan: Level, sql: String }
//    match_sql deleted (only the build path read it)

// 2. plan types stay, _0_types.rs
struct Level { pattern, rels: Vec<Rel>, conds: Vec<Cond> }
struct Rel   { from: CapRef, walk: Walk, rows: Rows, negated, stop, target }

// 3. deleted from hafley_scm
// _4_route.rs (route, Routed), _6_eval.rs (accepted, TABLES, INDEXES, register_regexp)
// QueryExt.scmpp, pair_routed_patterns, holds_for_candidate routing branch, MatchKey

// 4. build path, hafley_scm::build
fn build(lang, scm) -> Result<QueryExt, QueryExtError>
//    relation predicate (has?, has-ancestor?, has-parent?, precedes?, follows?, nth-child?)
//    -> QueryExtError::RelationPredicate { pattern, op }  "use ryii query --scmpp"

// 5. row producers move to ryi, src/bin/ryi/2_scmpp.rs (or sibling)
fn capture_rows(compiled, tree, src, sink)    // was scmpp::_5_rows
fn cst_rows(tree, sink)                       // was scmpp::_5_rows, over cst::walk_streaming

// 6. ryi owns SQLite: one DB per run, one writer, one executor
fn write_file(db, compiled, ...)  -> capture + edge rows
fn run_sql(db, compiled) -> rows             // INDEXES + regexp() move here

// 7. ryi query entry
query::run --scmpp FILE -> run_scmpp -> compile per lang -> write_file per file -> run_sql once

// 8. bundled query edit, queries/kotlin/scip.scm:122
// lambda_literal emits call.def kind=lambda for every lambda; #has-ancestor? removed
// readers of call.def kind=lambda checked for reliance on the filter

// 9. dependency edge
// hafley_scm rusqlite features drop "functions"; read side keeps rusqlite

// 10. tests
// hafley_scm/tests/_1_scmpp_compile.rs keeps SQL snapshots (prepare-only)
// neovim_predicates.rs: relation predicate -> RelationPredicate error
// sprefa-extract tests/195_scmpp_growth.rs unchanged
```
