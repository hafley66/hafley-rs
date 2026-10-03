# ryi graph fast/slow + scm++ stress lab, 2026-10-02

Branch `lab/20261002-ryi-graph-scmpp-stress`, base `origin/main` e3fe2f57. Binary:
`cargo build --release --bin ryii --features cli,ts-checker,typespec` (sprefa-extract workspace).
Full per-row tables: `plans/2026-10-02-ryi-graph-scmpp-stress-grid.html` (graph agreement 201 rows,
scm++ stress 32 rows, scm++ oracle 174 rows).

Corpora (read-only):

| corpus | source | files | bytes |
| --- | --- | --- | --- |
| hafley_scm | this worktree `crates/hafley_scm` (graph lab) | 166 | n/a |
| hafley-rs crates | this worktree `crates/` `*.rs` (scm++ stress) | 1294 | 12408065 |
| hafley-rxjs packages | `git archive` of origin/main 2145cb14 into `crates/sprefa-extract/bench/repos/hafley-rxjs-main` | 1519 | 6667668 |
| hafley-tsp | `git archive` HEAD e9ac456 into `bench/repos/hafley-tsp-head` (copied, not exercised: time went to the two above) | 166 | n/a |

Lab code: `crates/sprefa-extract/bench/labs/lab-20261002-ryi-graph-fast-vs-slow/` (`0_names.py` anchor
pick from fast stores, `1_run.py` runner into `results.db`, `2_agree.py`) and
`crates/sprefa-extract/bench/labs/lab-20261002-ryi-scmpp-stress/` (`fx/` probe fixtures,
`2_oracle.py`, `3_doc_examples.py`, `4_stress.py`, `5_grid.py`). The repo rule routes oracle
comparisons through `ryiii`; no `ryiii` bin or crate exists at e3fe2f57, so these scripts are lab-local.

## Findings

Severity: high = wrong answer / silent drop / unusable at corpus scale; medium = wrong in an edge
case, unbounded run, or a limit hit with a late error; low = message or UX.

| id | area | severity | status | repro | observed | expected |
| --- | --- | --- | --- | --- | --- | --- |
| G1 | graph --slow | high | fixed 71fe3fee | `cd bench/repos/hafley-rxjs-main && ryii graph --from mountGrid --slow --root . packages` (TS 7.0.2 project) | `ts_checker.mjs` throws (`no typescript package reachable`, or `ts.sys` undefined under TS 7); WARN on stderr; `0 edges`, exit 0. Same for `--call-path`/`--type-path`/`--flow-path` | non-zero exit naming the declined tier |
| G2 | graph fast | high | queue | `ryii graph --callers push --root crates/hafley_scm crates/hafley_scm` | 556 of 587 edges land on `read/lang/7b_rust_std_shim.rs` (`Vec::push` etc.); 1627 of 1755 Rust `--callers` fast-only edges in the lab target that file; `--uses String` binds 763 sites to it via `corpus_unique` | the shim is `include_str!`'d, owns no module in the crate; no binding |
| G3 | graph PATH#NAME | high | fixed f02798a5 | `ryii graph --callers atoms.rs#lookup ...` / `ryii graph --from ./crates/.../17_tree_entity_rows.rs#collect_items ...` | `--callers` matched canonical paths by equality (suffix forms: 0 rows); walks matched `Path::ends_with` (`./` and absolute forms: 0 rows); exit 0 both | `--help`: "files whose path ends with PATH", one rule for all arms |
| G4 | graph fast | medium | queue | `ryii graph --callers lookup` / `node_text` / `render` (lab anchors) | fast misses: `pub use crate::span::node_text as go_text` call sites (go.rs:1947); `strings.lookup(...)` through a `&Strings` param (101 slow-only); TS `controlsRegion.render(...)` on a returned object; TS `<Drawing />` of a nested function component; fast adds `parser.parse(...)` (tree_sitter) bound to `read/types.rs::parse` | slow-tier edges |
| G5 | graph --slow --uses | medium | queue | `ryii graph --uses String --slow --root crates/hafley_scm crates/hafley_scm` | slow `--uses` output equals fast on 12 of 12 anchors (both languages), including the 763 shim bindings of G2 | an independent checker answer, or a statement that `--uses` has no slow tier |
| G6 | graph --timeout | medium | queue | `ryii graph --from collect_items --slow --timeout 1 --root crates/hafley_scm crates/hafley_scm` | 53.8 s, exit 0 | the budget covers the resolve, or `--help` says it bounds only the question |
| G7 | graph | low | queue | `ryii graph --callers no_such_name_zzz ...`, `--callers nosuch.rs#lookup` | 0 rows, exit 0 | distinguishable from "declared, no callers" |
| G8 | graph --type-path | low | queue | `ryii graph --type-path FamilyBundle --root crates/hafley_scm crates/hafley_scm` | rows with `"to_name":null` (target `read/types.rs`) | a named target or no row |
| G9 | graph PATH#NAME | info | out of scope | `--callers 'crates\hafley_scm\src\atoms.rs#lookup'` on macOS | 0 rows (`\` is a filename byte on Unix) | n/a |
| S1 | scm++ SQL | high | fixed c0126dc5 | `ryii query --scmpp q.scm --pattern '*.rs' crates` with `((call_expression) @call (#has-ancestor? @call function_item))` | 205.7 s; `has?` and `has-parent?` over crates/ > 300 s; EXPLAIN: target root `SEARCH r1 USING INDEX scmpp_capture_node (pattern=? AND capture=?)`, a scan of every target root per walk row | index seek on content id; 26.7 s / 30.3 s / 16.8 s after |
| S2 | scm++ negation | medium | queue | `fx/z.rs` = `fn c() { g(); h(1); }`, `((arguments (integer_literal)? @n) @args (#not-contains? @n "9"))` | the `g()` match (no `@n`) is dropped; `#not-has-parent? @n ...` keeps it | doc: `#not-contains?` "is its complement"; `NOT (NULL)` drops the row. One-line candidate: `Cond::Not` lowers to `NOT COALESCE((...), 0)` |
| S3 | scm++ rows | medium | queue | `fx/q.rs`, `((function_item (parameters (parameter)* @p) body: (block (expression_statement (call_expression arguments: (arguments (integer_literal)* @n))))) @f)` | 15 rows for 2 matches: the LEFT JOIN per capture name emits the cartesian product of quantified captures | documented row shape for quantified captures |
| S4 | scm++ runtime | medium | queue | `fx/deep.rs` (200 nested blocks), `fx/deepeach5.scm` (5 nested `rows: each` has-ancestor) | > 300 s, no `--timeout` on `ryii query` | a budget flag like `graph --timeout` |
| S5 | scm++ limits | medium | queue | `fx/many63.scm` (63 captures); `fx/d30.scm` (30 nested levels) | `scm++ SQL: at most 64 tables in a join` (62 captures pass); `scm++ SQL: Expression tree is too large (maximum depth 1000)` (24 levels pass, 30 fail); both after every file is read | compile-time error naming the limit |
| S6 | scm++ memory | medium | queue | `ryii query --scmpp af.scm --pattern '*.rs' crates` (no `--sqlite`) | 4.61 GB peak RSS, 24.5 s for 12.4 MB of source; every run writes all CST nodes and edges (3,338,297 edge rows, 2.33 GB DB), a relation-free query included | CST rows only when a relation reads them; at 16 GB RAM the in-memory limit is near 40 MB of source |
| S7 | scm++ errors | low | queue | `((call_expression) @c (#has? @c identifier)))` | `syntax at byte 22: unbalanced close`; the extra `)` is at byte 44 | offsets in the query's coordinates (root text is re-scanned after predicates are cut) |
| S8 | scm++ errors | low | queue | `((call_expression) @c (#has? @c ((identifier) @i (#match? @c "(("))))` | `scm++ SQL: regex parse error` at SQL time, exit 2 | compile-time check (same-level `#match?` is checked by tree-sitter) |
| S9 | errors | low | queue | `stopBy: sideways`; `(#has? @a (x) @lit)`; `ryii query --query '(... (#inside? @c x))'` | `bad option stopBy: Word("sideways")`, `unexpected argument Capture("lit")`, `UnknownOperator("inside?")` (Rust Debug text) | plain text |
| S10 | query inputs | low | queue | `ryii query --scmpp q.scm crates/hafley_scm` (also `--query`) | `crates/hafley_scm/Cargo.toml: no language for this extension; pass --lang`, exit 2; `ryii graph` takes the same directory | skip non-source files in directory walks, or say `--pattern` is required |
| S11 | scm++ | low | documented | `(#has? @c nosuchkind)` | every file skipped with one diagnostic line, exit 0 | as documented |
| S12 | scm++ inputs | low | queue | 3000 random bytes as `fx/bin.rs` | no rows, no diagnostic, exit 0 | a diagnostic |
| S13 | tree-sitter | info | native | `fx/many10.scm` before anchoring: 10 unanchored `(parameter) @pN` over 80 params | > 120 s in tree-sitter matching (C(80,10) matches); plain `--query` has the same cost | n/a |

No panic and no hang without an input-driven cause was observed in any run: 402 graph runs (0
non-zero exits), 174 oracle cases twice, 32 stress runs, 19 doc examples, and the `fx/` probes.

## 1. graph fast vs slow

Anchors (71, `0_names.py` over the fast stores): Rust from `crates/hafley_scm` - collisions (6),
trait methods (4), generics (4), re-exports (3), singles (5), the 6 collisions again as
`PATH#NAME`, 6 types. TypeScript from hafley-rxjs `packages` - collisions (6), overloads (3), default
exports (2), barrels (3), JSX components (3), generics (3), singles (5), 6 `PATH#NAME`, 6 types.
Call anchors run `--callers`, `--from`, `--call-path`; type anchors `--uses`, `--type-path`; each
fast and `--slow`. Edge keys: `graph_edge` (from_path, from_name, from_line, to_path, to_name);
`graph_node` (path, name); `graph_path` (from_path, from_name, to_path, to_name). Runs used the
e3fe2f57 binary with `SPREFA_TS_CHECKER_TYPESCRIPT` pinned to a TypeScript 5.9.3 `typescript.js`
(the TS 7 project compiler fails G1).

| language | arm | anchors | agree | fast_only | slow_only | both_empty | identical_sets |
| --- | --- | --- | --- | --- | --- | --- | --- |
| rust | callers | 28 | 1106 | 1746 | 324 | 0 | 9 |
| rust | from | 28 | 416 | 127 | 103 | 12 | 14 |
| rust | call-path | 28 | 400 | 143 | 119 | 12 | 14 |
| rust | uses | 6 | 2467 | 0 | 0 | 0 | 6 |
| rust | type-path | 6 | 13 | 7 | 146 | 1 | 2 |
| ts | callers | 31 | 2650 | 10 | 36 | 0 | 29 |
| ts | from | 31 | 845 | 433 | 191 | 11 | 18 |
| ts | call-path | 31 | 808 | 470 | 228 | 11 | 18 |
| ts | uses | 6 | 832 | 0 | 0 | 0 | 6 |
| ts | type-path | 6 | 11 | 10 | 0 | 5 | 5 |

| language | tier | arm | mean_seconds | max_seconds |
| --- | --- | --- | --- | --- |
| rust | fast | callers | 0.5 | 0.5 |
| rust | slow | callers | 6.7 | 11.5 |
| rust | slow | from | 15.8 | 16.7 |
| rust | slow | type-path | 15.5 | 16.0 |
| ts | fast | callers | 1.3 | 1.3 |
| ts | slow | callers | 20.2 | 27.4 |
| ts | slow | from | 15.4 | 15.9 |
| ts | slow | uses | 21.4 | 22.0 |

Disagreement classes (sampled; counts are keys from `results.db`):

| class | language | arms | keys | verdict |
| --- | --- | --- | --- | --- |
| std method/type bound to `7b_rust_std_shim.rs` (G2) | rust | callers, from, call-path, type-path | 1627 callers fast-only, 60 from, 60 call-path, 6 type-path | fast wrong |
| receiver typed through param/field (`strings.lookup`) | rust | callers | 101 slow-only (`lookup`) | fast misses |
| `pub use X as Y` alias call (`go_text` -> `node_text`) | rust | callers | 6 slow-only (`node_text`) | fast misses |
| external method bound by name (`parser.parse` -> `read/types.rs::parse`) | rust | callers | 19 fast-only (`parse`) | fast wrong |
| struct construction (`Span(..)`, `NodeRef`) counted as a call | rust | callers | 59 + 38 fast-only, slow 0 | slow omits constructor calls (scope) |
| trait-method walks (`resolve`, `build`) | rust | from, call-path | 45 fast-only, 83 slow-only (`resolve` from) | mixed: slow-only go/kotlin helpers reached through trait dispatch |
| type-path field/aux types (`FamilyBundle` -> `CallFAux`, ...) | rust | type-path | 146 slow-only | fast misses associated/generic arguments |
| workspace package import (`@hafley66/signal-grid/react` -> `GridView`) | ts | from, call-path | most of `MarkdownTable` 180 fast-only | slow wrong here: the archive copy has no pnpm workspace links, the checker cannot resolve the package |
| method on a returned object (`controlsRegion.render`) | ts | callers | 36 slow-only (`render`) | fast misses |
| JSX of a nested function component (`<Drawing />`) | ts | from, call-path | part of `mount` 104 slow-only | fast misses |
| `.rs` files inside the TS corpus (`grid` in grapht wgpu tests) | ts | callers | 26 fast-only | out of scope for the TS checker; after 71fe3fee the same corpus fails with `tier.rust-analyzer declined` |
| `--uses` | both | uses | 0 / 0 | slow = fast (G5) |

PATH#NAME and flags (e3fe2f57 binary, Rust corpus):

| anchor form | callers_rows | from_rows | after f02798a5 |
| --- | --- | --- | --- |
| `crates/hafley_scm/src/atoms.rs#lookup` | 58 | n/a | 58 |
| `./crates/.../atoms.rs#lookup` | 58 | n/a | 58 |
| absolute `#lookup` | 58 | n/a | 58 |
| `atoms.rs#lookup` | 0 | n/a | 58 |
| `crates/.../17_tree_entity_rows.rs#collect_items` | n/a | 24 | 24 |
| `./crates/.../17_tree_entity_rows.rs#collect_items` | n/a | 0 | 24 |
| absolute `#collect_items` | n/a | 0 | 24 (test: `t_149::path_anchor_forms_agree_across_arms`) |
| `17_tree_entity_rows.rs#collect_items` | n/a | 24 | 24 |
| trailing slash `.../atoms.rs/#lookup` | 58 | 24 | 58 |
| `#lookup`, `x.rs#`, `a#b#c` | exit 2 `requires NAME or FILE#NAME` | same | same |
| `Strings.lookup` | exit 2 `Class.method is unsupported` | 0 rows | same |
| `--timeout 0` | exit 2 (clap range) | same | same |

## 2. scm++

### Correctness

- Oracle (`2_oracle.py`): 6 kind pairs x {has, has-ancestor, has-parent, precedes, follows} x
  {default, `stopBy: neighbor`, `stopBy: (block)`, `field:`, `not-`} plus `nth-child` N in 1..3 with
  and without `of`: 174 cases over `crates/hafley_scm/src/lang` (26 files). The oracle takes level
  matches from the run's `capture` rows and the tree from its `edge` rows and recomputes each
  relation by parent/child/named-sibling walks in Python. 174 of 174 agree (114 non-empty, 175,706
  rows), on e3fe2f57 and on f02798a5. It checks the SQL lowering, not the CST writer.
- Hand probes on small fixtures (`fx/`): 4-level nesting mixing `has-ancestor`/`not-has`/`has` with
  cross-level `#eq?`; same-name capture identity; `stopBy` pattern on precedes/follows (inclusive);
  `nth-child` with and without `of`; `field:` on `has-ancestor`; alternation root `[(a) (b)] @x`;
  anchors `.`; kind lists; quantifiers `? * +`; CRLF + non-ASCII identifiers; syntax-error file;
  empty file; two files with one content id (rows for both paths). All matched manual reasoning
  except S2 and S3.
- The 19 `scheme` examples of `docs/2_scm-with-ast-grep-relations-20260920.md` reproduce their
  printed rows (`3_doc_examples.py`).
- Error probes (25 queries in one loop): unbalanced open/close, unterminated string, empty and comment-only query,
  unknown predicate, unknown capture, capture used before bind, unknown kind/field/option, duplicate
  option, `has-parent` with `stopBy`, `nth-child` 0/-1/`of` without target, `@__root`, a name bound
  by two exported levels, two top-level patterns, invalid regex at both levels. All exit 2 with a
  message except unknown kind (exit 0 + skip diagnostic, documented); message defects S7-S9.

### Stress (wall time, peak RSS, DB size, rows)

`4_stress.py`, `--sqlite` kept, one query per run. e3fe2f57 runs overlapped the graph lab (one
other ryii process), f02798a5 runs had the machine alone; the crates/ corpus is this worktree, so
the commits shift a few row counts.

| binary | corpus | query | wall_seconds | peak_rss_mb | db_mb | edge_rows | result_rows |
| --- | --- | --- | --- | --- | --- | --- | --- |
| e3fe2f57 | hafley-rs crates | plain_calls | 18.5 | 1040 | 2334 | 3337853 | 131198 |
| e3fe2f57 | hafley-rs crates | ancestor_fn | 205.7 | 2123 | 2341 | 3337853 | 130764 |
| e3fe2f57 | hafley-rs crates | has_return | timeout 300 | n/a | n/a | n/a | n/a |
| e3fe2f57 | hafley-rs crates | parent_stmt | timeout 300 | n/a | n/a | n/a | n/a |
| e3fe2f57 | hafley-rs crates | nested3_each | timeout 300 | n/a | n/a | n/a | n/a |
| e3fe2f57 | hafley-rxjs packages | ancestor_fn | 95.8 | 1424 | 1586 | 1971181 | 23694 |
| e3fe2f57 | hafley-rxjs packages | has_return | 94.6 | 1140 | 1494 | 1971181 | 2467 |
| e3fe2f57 | hafley-rxjs packages | parent_stmt | 89.2 | 1133 | 1605 | 1971181 | 18841 |
| f02798a5 | hafley-rs crates | plain_calls | 17.7 | 1574 | 2334 | 3338297 | 131222 |
| f02798a5 | hafley-rs crates | ancestor_fn | 26.7 | 2566 | 2342 | 3338297 | 130788 |
| f02798a5 | hafley-rs crates | ancestor_each | 22.7 | 2256 | 2353 | 3338297 | 131021 |
| f02798a5 | hafley-rs crates | has_return | 30.3 | 2800 | 2197 | 3338297 | 1635 |
| f02798a5 | hafley-rs crates | parent_stmt | 16.8 | 1668 | 2339 | 3338297 | 12254 |
| f02798a5 | hafley-rs crates | precedes_neighbor | 15.5 | 1626 | 2221 | 3338297 | 15193 |
| f02798a5 | hafley-rs crates | nth_of | 16.6 | 1627 | 2211 | 3338297 | 6956 |
| f02798a5 | hafley-rs crates | nested3_each | 230.1 | 2999 | 2923 | 3338297 | 44082 |
| f02798a5 | hafley-rxjs packages | ancestor_fn | 17.1 | 2116 | 1586 | 1971181 | 23694 |
| f02798a5 | hafley-rxjs packages | has_return | 15.7 | 2025 | 1494 | 1971181 | 2467 |
| f02798a5 | hafley-rxjs packages | parent_stmt | 11.4 | 1638 | 1605 | 1971181 | 18841 |
| f02798a5 | hafley-rxjs packages | nested3_each | 56.1 | 3244 | 2150 | 1971181 | 49172 |

Single-file and query-shape limits (`fx/deep.rs`, 200 nested blocks):

| case | e3fe2f57 | f02798a5 |
| --- | --- | --- |
| 5 nested `has-ancestor` levels, rows first | 0.68 s | 0.73 s |
| 20 nested levels, rows first | 88.4 s | 9.8 s |
| 24 nested levels | n/a | ok |
| 30, 40, 80 nested levels | n/a | `Expression tree is too large (maximum depth 1000)` |
| 5 nested levels, `rows: each` | > 120 s | > 300 s (result is the product of ancestor counts) |
| 62 / 63 exported captures | ok / `at most 64 tables in a join` | same |
| in-memory (no `--sqlite`) ancestor_fn over crates | n/a | 24.5 s, 4.61 GB RSS |

Where the time goes (`sample`):
- e3fe2f57 has-ancestor on hafley_scm: 9839 of 12539 samples in `run_sql` -> `sqlite3_step`; the
  plan showed the target root searched by `(pattern, capture)` only. Adding the content-id equality
  takes the same SQL from 41.2 s to 1.6 s in the sqlite3 CLI (`walk.sql` vs `walk_c.sql` in
  `bench/repos/stress-db/`); `AS MATERIALIZED` on the walk CTE changes nothing (41.1 s).
- f02798a5 nested3_each on crates: 8353 samples in `run_sql`, 6064 under
  `sqlite3BtreeIndexMoveto` -> `sqlite3VdbeRecordCompareWithSkip` / `memcmp`: index seeks on text
  keys (`_content_id` is a 71-byte `blake3:` string, `_input_path` text).
- Relation-free queries spend their time writing CST rows (3.34 M edges for crates/).

## 3. `--query` and bundled queries

- 23 `.scm` files outside `target/` and `bench/`: 0 contain `#has?`, `#has-ancestor?`, `#has-parent?`,
  `#precedes?`, `#follows?`, `#nth-child?`, `#contains?` (or `not-` forms, or the older `#inside?`).
  No Rust source outside `hafley_scm/src/scmpp` embeds them. Docs: only the scm++ guide; plans and
  `sprefa-lab-scopegraph/REPORT.md` mention the old host predicates as history.
- `ryii query --query` with each relation and `#contains?` (plain, `not-`, nested inside a pattern,
  inside an alternation, in the second top-level pattern): exit 2,
  `query (rust): pattern N: #OP runs only under ryii query --scmpp`, N = top-level pattern index.
  `#inside?` and unknown predicates: exit 2 `UnknownOperator("inside?")` (S9).

## Fixes

| commit | finding | change | test |
| --- | --- | --- | --- |
| c0126dc5 | S1 | `_3_lower.rs`: the walk join and the one-edge parent join compare the content id with the target (`same` instead of `same_node`), so `scmpp_capture_node` seeks | 10 SQL snapshots in `hafley_scm/tests/_1_scmpp_compile.rs` pin the `cid = r._content_id` term; 17/17 pass; oracle 174/174 unchanged; `t_194`/`t_195` pass |
| 71fe3fee | G1 | `hafley_scm/src/read/2_slow.rs::checker_facts`: request witness rows, return `CheckerUnavailable` listing every `tier.*` decline | `t_172_graph_slow::slow_walk_fails_when_the_typescript_checker_declines` (pin to a missing `typescript.js`, expects `ryi slow: tier.tsc declined`) |
| f02798a5 | G3 | new `src/0b_graph_anchor.rs::anchor_path_matches`, used by `--callers` and `named_starts` (both former rules removed) | unit `graph::anchor::tests::relative_forms_match_by_suffix_and_absolute_by_file`; integration `t_149_graph_callers_ts::path_anchor_forms_agree_across_arms` |

Gates run: `cargo test -p hafley_scm --test _1_scmpp_compile` (root workspace) 17/17;
`cargo test --features cli,ts-checker --test all -- slow graph` 120/120;
`-- t_149 t_172 t_194 t_195` 28/28; `--bin ryii -- anchor graph` 5/5. The full
`cargo test --features cli` gate was not run.

71fe3fee changes behavior on mixed corpora: hafley-rxjs `packages` holds `.rs` files outside any
crate graph, so `graph --slow` walks there now fail with `tier.rust-analyzer declined: ... owns no
module in the loaded crate graph`, where the lab's e3fe2f57 runs dropped those files silently.

## Queue (ranked)

1. G2 fast binds to files outside the crate module graph (`7b_rust_std_shim.rs`): 1627 wrong
   callers edges in one crate; `corpus_unique`/receiver binding should consult `RustModuleIndex`
   unmodulated files.
2. S6 CST rows for relation-free queries and in-memory RSS (4.6 GB for 12.4 MB): write CST rows only
   when a level has a relation; size the in-memory limit.
3. S2 `not-` complement on absent captures (`NOT COALESCE`, one line + `t_194` row).
4. G4 fast recall gaps: `use ... as` aliases, param/field-typed receivers, returned-object methods,
   nested JSX components.
5. S4 + G6 budgets: `ryii query --scmpp --timeout` (SQLite interrupt as in graph), graph timeout over
   the resolve.
6. S5 compile-time checks for the 64-table join and expression depth limits.
7. Remaining SQL cost (nested3_each 230 s on crates/): integer content ids / interned paths for the
   capture and edge keys.
8. G5 `--uses --slow` independence; S3 document or change quantified-capture row expansion.
9. S7, S8, S9, S10, S12, G7, G8 messages and input handling.
10. Lab gap: rerun the TS walk comparison with a workspace-linked copy (pnpm install) so slow can
    resolve `@hafley66/*` imports, and add hafley-tsp.
