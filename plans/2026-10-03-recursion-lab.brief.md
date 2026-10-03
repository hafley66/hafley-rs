# Brief: lab-20261003-recursion-pgq-gritql-sprefa

Compare ways to express RECURSION over ryi's facts (the part scm++ deliberately does not do):
SQL/PGQ (SQL:2023) on DuckDB (DuckPGQ) and on PostgreSQL if a PGQ build is available,
plain `WITH RECURSIVE` on SQLite and DuckDB, GritQL recursive patterns, and a sprefa rule.
Same inputs, same oracle, same metrics. Findings decide where chains and graph paths are
written; no product code changes in this lab.

## Context (read first)

- Boundary: `.claude/skills/2026-10-03-ryi-sprefa-boundary/SKILL.md`. Open recursion over graphs
  belongs to sprefa; ryi provides facts from a closed store; scm++ stays non-recursive. This lab
  measures candidate notations/engines for that recursion; it changes no ownership.
- Store layout: `plans/2026-10-03-scmpp-interned-preorder-store.md` (tables `scmpp_dict_*`,
  `scmpp_node(file, pre, last, parent, depth, sib, idx, kind, field, start, end, named)`,
  `scmpp_capture`). Strings interned; ids only in rows (`.claude/skills/2026-10-03-sqlite-interning`).
- Prior labs this one extends (reuse their code, do not copy-paste-diverge):
  - `crates/sprefa-extract/bench/labs/lab-20261002-ryi-scmpp-stress/` (Python oracle pattern
    `2_oracle.py`, fixtures `fx/`).
  - `crates/sprefa-extract/bench/labs/lab-20261003-scmpp-sqlite-vs-duckdb/` (store export,
    SQLite vs DuckDB CLI timing harness `2_engines.sh`; its `db/` must be regenerated, it was
    removed with the worktree).
  - `crates/sprefa-extract/bench/labs/lab-20261002-ryi-graph-fast-vs-slow/` (Rust anchors and
    call edges for T3).
- sprefa owns engine shootouts (dd, sqlite_ivm, pg_ivm, DBSP, DuckDB+OpenIVM:
  sqlite_ivm `plans/2026-10-03-duckdb-vs-dd.brief.md`). This lab benchmarks NO incremental/IVM
  arms; one-shot queries only. The sprefa arm is run by the sprefa session on this lab's exported
  inputs (see Arm S).
- Repo rules: Rust and TypeScript corpora only (Rust only here); bench DBs under
  `crates/sprefa-extract/bench/`, never /tmp or caches; tables 1NF, full-word columns; large tables
  to an HTML grid (pattern: `scripts/bench_grid.py`); one cargo command at a time; RCARGO_OFF=1.
  DuckDB / Postgres / grit are CLI tools only, never linked into any binary.

## Tasks (each has an oracle)

| task | question | shape | oracle |
| --- | --- | --- | --- |
| T1 chain | every link of every method chain: from an outermost `call_expression`, follow child edges whose field is `function` or `value`, collect `call_expression` nodes reached | tree-local recursion, edge-property filter per step | Python walk over `scmpp_node` (parent, field) |
| T2 self-call | functions whose body calls themselves | fixed depth, correlated (scm++ already does it) | existing scm++ result + Python walk |
| T3 reachability | all functions reachable by calls from a seed, not entering extern crates | open recursion over a cyclic graph across files | Python BFS over the exported call-edge table |
| T4 shortest call path | shortest call path seed -> target | graph shortest path | Python BFS |

Seeds/targets for T3/T4: 10 Rust anchors from lab-20261002-ryi-graph-fast-vs-slow, plus
`crates/hafley_scm/src/lib.rs#build` and `crates/sprefa-extract/src/0_graph.rs#run`.

## Inputs (exported once, shared by every arm)

- Corpora: (a) `crates/hafley_scm/src` + `crates/sprefa-extract/src` (small), (b) hafley-rs `crates/` (large).
- CST tables: `ryii query --scmpp <relation query> --sqlite db/cst-<corpus>.db` (a query with one
  relation so node rows are written).
- Call edges: `ryii --resolve --kinds cst,call --sqlite db/calls-<corpus>.db` (fast tier, `resolved_edge`);
  if `graph --slow` demand walk has merged by then, also a slow-tier edge export; record which.
- Normalized edge table `call_edge(src_fn_id, dst_fn_id, extern)` and `fn(id, path_id, name_id)` with
  interned ids, derived by `0_export.sh` in SQL.
- Parquet copies of every table (DuckDB `COPY ... (FORMAT parquet)`), for DuckDB and the sprefa arm.

## Arms

| arm | engine | T1 | T2 | T3 | T4 |
| --- | --- | --- | --- | --- | --- |
| P1 DuckPGQ | DuckDB CLI + `install duckpgq from community` | `MATCH (o)-[e WHERE e.field IN (...)]->{1,}(l)` over `scmpp_node` as vertex + edge table | n/a (non-recursive; scm++ baseline) | `MATCH (s)-[:calls]->{1,}(t)` | `ANY SHORTEST` |
| P2 PostgreSQL PGQ | only if a PostgreSQL build with `GRAPH_TABLE` installs via Homebrew without building from source; else record `not_available` with the version checked | same SQL as P1 where the dialect allows | n/a | same | same |
| R1 recursive CTE, SQLite | sqlite3 CLI | `WITH RECURSIVE` over parent/field | n/a | `WITH RECURSIVE` with cycle guard (UNION) | BFS by depth |
| R2 recursive CTE, DuckDB | DuckDB CLI | same SQL as R1 | n/a | same (+ `USING KEY` if it changes results or time) | same |
| G GritQL | `grit` CLI (install via the documented installer; record version) over the same Rust files | recursive named pattern | `fn $name(..) { $body }` where `$body <: contains $name(..)` | n/a (single file, no cross-file edges): record `not_expressible` | n/a |
| S sprefa rule | run by the sprefa session (claude-44) with dl8 over the Parquet / SQLite exports | rule | rule | rule | rule |
| B scm++ baseline | ryii release | fixed-depth unrolled chain for depth 1..5 (shows the depth cap) | `#has?` + cross-level `#eq?` | n/a | n/a |

Also in R2/P1: run once with `PRAGMA threads=1` and once default, to separate DuckDB's range-join /
graph operators from parallelism (open question from the sprefa session).

Verify before writing: whether DuckPGQ accepts a `WHERE` on edge properties inside a quantified
path. If not, record it and use one edge table per field (`child_function`, `child_value`) with a
label alternation, and note the difference.

## Metrics (one row per arm x task x corpus x thread setting)

`arm, task, corpus, threads, rows, oracle_rows, agree, arm_only, oracle_only, wall_s, peak_rss_mb,
setup_s (load/export/install), notation_lines (lines of the query), status (ok | not_expressible |
not_available | error), note`. Correctness first: any `arm_only`/`oracle_only` > 0 is explained
row by row (sample 5) before timing is reported.

## Net new files

All under `crates/sprefa-extract/bench/labs/lab-20261003-recursion-pgq-gritql-sprefa/`:

| file | role |
| --- | --- |
| `README.md` | hypotheses, how to run, versions of every CLI used |
| `0_export.sh` | builds `db/cst-*.db`, `db/calls-*.db`, derives `call_edge`/`fn`, writes Parquet |
| `1_oracle.py` | T1-T4 oracles (reuses the stress lab's oracle style); writes `oracle_<task>_<corpus>.tsv` |
| `2_scmpp.sh` + `2_chain_depth{1..5}.scm`, `2_self_call.scm` | arm B |
| `3_duckpgq.sql`, `3_duckpgq.sh` | arm P1 (both thread settings) |
| `4_postgres_pgq.sql`, `4_postgres_pgq.sh` | arm P2 or the `not_available` record |
| `5_recursive_cte.sql`, `5_recursive_cte.sh` | arms R1, R2 |
| `6_gritql/{chain.grit,self_call.grit}`, `6_gritql.sh` | arm G |
| `7_sprefa/{chain.dl7,reach.dl7,shortest.dl7,README.md}` | inputs + instructions for arm S; results file the sprefa session returns (`7_sprefa/results.tsv`) |
| `8_compare.py` | joins arm outputs with oracles -> `results.tsv`; writes `grid.html` (bench_grid pattern) |
| `results.tsv`, `grid.html` | committed results |
| `db/` | gitignored stores, Parquet, scratch |

Plus a "Results" section appended to this brief, and one row per finding in
`crates/sprefa-extract/bench/dogfood.tsv` if the lab edits the tree with ryi tools (it should not).

## Deliverable

Commit the lab directory and results on branch `lab/20261003-recursion-pgq-gritql-sprefa`; no
product code changes. Report: results table summary (correctness then time/RSS), notation
comparison for T1 and T3 (the query text per arm), DuckPGQ edge-filter finding, threads=1 finding,
which arms were `not_expressible` / `not_available`, and a ranked recommendation for where chain
(T1) and graph-path (T3/T4) queries should be written.

Time box: one pass. Stop and report if an arm cannot be installed or run within ~20 minutes of
trying; record it as `not_available` with the exact error.

## Results (2026-10-03)

Lab: `crates/sprefa-extract/bench/labs/lab-20261003-recursion-pgq-gritql-sprefa/` (`results.tsv`, `grid.html`,
README for inputs and versions). Corpora: small = hafley_scm/src + sprefa-extract/src (1,030,736 CST rows),
crates = hafley-rs `crates/` (3,439,810 CST rows). T3/T4 run on both tiers: fast (`ryii --resolve`, 10808 / 28130
edges) and slow (`ryii graph --slow --sqlite`, 6588 / 20132 edges). Oracle rows: T1 8302 / 35484, T2 136 / 210,
T3 fast 655 / 674, T3 slow 37 / 37, T4 7 fast and 6 slow pairs.

### Correctness (arm_only / oracle_only = 0 unless listed)

| arm | T1 | T2 | T3 | T4 |
| --- | --- | --- | --- | --- |
| R1 SQLite `WITH RECURSIVE` | exact | n/a | exact | exact |
| R2 DuckDB `WITH RECURSIVE` (+ `USING KEY` variant) | exact | n/a | exact | exact |
| P1 DuckPGQ | error: timeout 600 s on small (also 1050 s before kill; `{1,3}` bounded form > 300 s) | n/a | exact | exact |
| P2 PostgreSQL PGQ | not_available | n/a | not_available | not_available |
| G GritQL | single process dies (stack overflow); per file: 67/176 (small), 235/809 (crates) files die; surviving files 2700/2702 and 16289/16292 link rows agree | exact | not_expressible | not_expressible |
| B scm++ fixed depth | depth 5 misses 310 (small) / 2128 (crates); arm_only 0 | exact | n/a | n/a |
| S sprefa | pending | pending | pending | pending |

Disagreements explained:
- G T1: link rows only (outer not emitted; the forms that collect every link per outer, `bubble` and list `+=`,
  overflow or hang). The 2-3 arm_only/oracle_only pairs are one grammar difference: grit's tree-sitter-rust gives the
  call inside `text[1..text.len()]` the span `1..text.len()`.
- B T1: misses are links the unrolled `function: (field_expression value: ...)` shape does not reach: links under a
  turbofish (`generic_function`), chains deeper than 5 calls, call-of-call, several field hops, and the
  `#not-has-parent?` outer filter. Rows only grow with depth (small 5528 -> 7992 for depth 1 -> 5).

### Time and memory (wall_s / peak_rss_mb, threads=1 then default where both ran)

| arm | T1 small | T1 crates | T4 crates fast |
| --- | --- | --- | --- |
| R1 | 0.44 / 66 | 1.67 / 106 | 3.23 / 12 |
| R2 | 0.18, 0.06 / 57, 76 | 0.51, 0.16 / 128, 139 | 3.43, 12.88 / 290, 289 |
| R2_key (`USING KEY` + `recurring`) | - | - | 0.02, 0.03 / 51, 71 |
| P1 | > 600 / 1739 | not run | 0.07, 0.06 / 165, 174 |
| G (per file) | 42.72 / 47 | 169.73 / 47 | - |
| B depth 1..5 summed | 8.15 / 113 | 27.49 / 241 | - |

T3 runs are 0.00-0.33 s for every arm (12 seeds reach at most 674 functions). Plan sensitivity: SQLite T1 without
`ANALYZE` 118 s (small); DuckDB T1 with a decode join back to `scmpp_node` 108 s (crates) before the CTE carried the
spans itself.
