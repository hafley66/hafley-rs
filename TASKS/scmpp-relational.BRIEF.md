# scm++ relational predicates: ast-grep/CSS relations with s-expr args, siblings, options (burndown row 9)

Repo hafley-rs, crate crates/hafley_scm. Base: main.
Read: plans/2026-10-01-burndown.md row 9; plans/2026-10-01-scm-sql-tsp-scope.md (predicate table lines 13-20, gaps lines 103-121, 226, 239);
crates/hafley_scm/src/types/{walk.rs,stop.rs,predicate.rs}; crates/hafley_scm/src/walk/*; 
crates/hafley_scm/src/pipeline/split_predicates_into_kind_queries/{parse_into_predicate.rs,ts_read_general_predicates.rs};
crates/hafley_scm/src/pipeline/run_over_file_tree/test_predicates_per_candidate.rs; the servo lab merged at 100fdde0 (CSS selectors over tree-sitter CST);
docs for .scm relations from df1239fd. Reference semantics: ast-grep relational rules (inside, has, precedes, follows; stopBy neighbor|end|<rule>; field)
and CSS combinators (descendant, `>`, `+`, `~`, :has(), :nth-child()).

## Today
Walks: has / has-ancestor / has-parent with `@c kind+ [neighbor|end]`, kind-name args only. TODO in types/walk.rs:4-5 and types/stop.rs:4:
Precedes, Follows, NthChild, ByteRange, stopBy: rule.

## Task
1. Sibling relations: `#precedes?` / `#follows?` (+ `#not-…`) with `neighbor` (adjacent named sibling, CSS `+`) and `end` (any earlier/later sibling, CSS `~`).
2. `#nth-child?` (+ not): 1-based index among named siblings; optional `of kind`.
3. S-expression args: every relation accepts a nested tree-sitter pattern in place of the kind list, e.g.
   `(#has-ancestor? @c (function_item name: (identifier) @fn))`; the relation holds when a related node matches the pattern. Validate pattern node kinds
   against the grammar like kind names today (unknown = error).
4. Options: `stopBy` = neighbor | end | (pattern) — walk stops at the first node matching the stop pattern; `field: name` — the related node must sit in that field.
5. A relation with a pattern arg may bind captures from the related node (closes the plan's "enclosing fn binds no ancestor capture" gap, line 121).
6. Tests in crates/hafley_scm/tests: one table-driven file, one row per relation x option, fixture Rust and TS sources, expected match lists as inline snapshots.
   Each relation also gets the equivalent servo CSS selector in the same row as a comment (row 11 will turn those into an oracle; do not build the oracle).
7. Update the .scm relations docs page with the new predicates.

Rules:
- CODE COMPLETE ONLY. Do not run cargo build, cargo test, cargo check. The coordinator runs gates, one at a time. Machine load. CARGO_BUILD_JOBS=4 if ever told to build.
- Other lanes edit crates/hafley_scm/src/read/lang/ts*.rs and crates/sprefa-extract in parallel. Stay in types/, walk/, pipeline/split_predicates_into_kind_queries/,
  pipeline/run_over_file_tree/, tests/, docs. No drive-by renames or formatting.
- Grammar for the predicate args: use tree-sitter's own query parser for nested patterns; do not write a bespoke s-expression parser.
- Numbered file naming per repo convention (`_N_name.rs`, no #[path]). No hand-written files labelled generated. No push. No Python. No `boop beep scream`.
- Commit per relation; messages end `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`.
- On done: REPORT.md section: per relation the syntax, semantics, CSS equivalent, test row names, and the cargo test filter for the coordinator.
