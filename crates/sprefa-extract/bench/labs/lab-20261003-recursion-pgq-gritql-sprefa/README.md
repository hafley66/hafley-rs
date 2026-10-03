# lab-20261003-recursion-pgq-gritql-sprefa

Brief: `plans/2026-10-03-recursion-lab.brief.md` (results section appended there). One-shot recursion over ryi facts
in SQL/PGQ (DuckPGQ), PostgreSQL PGQ, `WITH RECURSIVE` (SQLite, DuckDB), GritQL, a sprefa rule, and the scm++
fixed-depth baseline. Same inputs, same oracle, same metrics. No product code changes.

## Hypotheses

1. T1 (method-chain links) is tree-local recursion with an edge filter per step; a recursive CTE over
   `scmpp_node(parent, field)` answers it exactly; scm++ needs one fixed-depth file per depth and never closes.
2. T3/T4 (call reachability / shortest path) are open recursion on a cyclic graph; every graph-capable engine
   answers them exactly, and their cost depends on frontier deduplication.
3. DuckPGQ path patterns fit T3/T4 (few sources) and do not fit T1 (all-pairs within a forest).
4. GritQL expresses T2 and a T1 projection, single-file only; T3/T4 are not expressible.

## Run

```
cd crates/sprefa-extract && RCARGO_OFF=1 cargo build --release --bin ryii --features cli,ts-checker,typespec
LAB=crates/sprefa-extract/bench/labs/lab-20261003-recursion-pgq-gritql-sprefa
$LAB/0_export.sh                 # ryii stores -> db/lab-<corpus>.{db,duckdb}, db/parquet-<corpus>/
python3 $LAB/1_oracle.py small; python3 $LAB/1_oracle.py crates   # db/oracle/*.tsv, t4_pair
$LAB/2_scmpp.sh; $LAB/5_recursive_cte.sh
JOBS="T3:fast T3:slow T4:fast T4:slow" $LAB/3_duckpgq.sh
JOBS=T1:cst THREADS=default TIMEOUT=600 $LAB/3_duckpgq.sh small
$LAB/4_postgres_pgq.sh; TIMEOUT=1800 $LAB/6_gritql.sh
python3 $LAB/8_compare.py        # results.tsv, grid.html, db/diff/<run>.txt
```

`0_measure.sh` is sourced by the arm scripts: `/usr/bin/time -l` + `timeout`, one row per run in `db/runs.tsv`,
output in `db/out/<run>.tsv`. Arm S writes `7_sprefa/out/` and `7_sprefa/runs.tsv` (see `7_sprefa/README.md`).

## Inputs

| corpus | paths | scmpp_node rows | fn | call_edge_fast | call_edge_slow |
| --- | --- | --- | --- | --- | --- |
| small | `crates/hafley_scm/src`, `crates/sprefa-extract/src` | 1030736 | 5124 | 10808 | 6588 |
| crates | `crates` | 3439810 | 15631 | 28130 | 20132 |

- CST: `ryii query --scmpp db/cst.scm --sqlite db/cst-<corpus>.db` with `((identifier) @id (#has-ancestor? @id source_file))`
  (one relation, so every node row is written; identifier text lands in `scmpp_dict_text` for T2).
- Fast tier: `ryii --resolve --kinds cst,call --sqlite`, table `resolved_edge`.
- Slow tier: `ryii graph --slow --from crates/hafley_scm/src/lib.rs#build --root . --sqlite`; its store holds
  `resolved_edge` rows for the whole input set (rust-analyzer `checker_resolve` / `name_resolve`, plus `call|scip`
  rows on small). Export cost: small 55 s / 3304 MB, crates 75 s / 3154 MB.
- Function identity is (path, name), the key `ryi graph` prints; same-name functions in one file merge.
  `extern = 1` when the callee is in a std/core shim file (`*_rust_*_shim.rs`); neither tier writes edges to files
  outside the corpus. T3/T4 traverse `extern = 0` edges only.
- Seeds: the six `collision_path` anchors of lab-20261002-ryi-graph-fast-vs-slow, four of its bare names
  path-qualified (`span.rs#node_span`, `0_request_root.rs#io_path`, `15_syntax.rs#parse_rust_file`,
  `types.rs#corpus_defs`), `crates/hafley_scm/src/lib.rs#build`, `crates/sprefa-extract/src/0_graph.rs#run`.
- T4 pairs: per seed and tier, the reachable function with the largest BFS depth (ties: smallest path, name).
- `0_export.sh` runs `ANALYZE` on the SQLite store. Without statistics SQLite plans the recursive step of T1 as
  a per-file primary-key range scan: 118 s on small, 0.35 s after `ANALYZE`.

## Task definitions (oracle = `1_oracle.py`)

- T1: from each `call_expression`, follow child edges whose field is `function` or `value`; every
  `call_expression` reached in >= 1 step is a link; outer = an origin that is itself no link. Row: path, outer span,
  link span. The tree walk imports `Tree` from `lab-20261002-ryi-scmpp-stress/2_oracle.py`.
- T2: `function_item` whose `body` has a `call_expression` descendant whose `function` child is an `identifier`
  with the function's name text. Row: path, function span.
- T3: functions reached from the seed by >= 1 edge. Row: anchor, path, name.
- T4: BFS length seed -> target. Row: anchor, target path, target name, length.

## Versions

| tool | version | install |
| --- | --- | --- |
| ryii | worktree base 7b2588f5, release, features cli,ts-checker,typespec | cargo |
| DuckDB CLI | v1.5.4 (08e34c447b) | GitHub release `duckdb_cli-osx-arm64.zip` into `db/bin`; Homebrew 1.5.5 has no duckpgq build (community repo HTTP 404 for v1.5.5/osx_arm64) |
| duckpgq | f386a6c | `INSTALL duckpgq FROM community` |
| SQLite CLI | 3.53.4 | Homebrew `sqlite` |
| grit | 0.1.1 | `docs.grit.io/install` returns HTTP 404; GitHub release `biomejs/gritql` v0.1.0-alpha.1743007075 `grit-aarch64-apple-darwin.tar.gz`, sha256 verified, into `db/bin` |
| PostgreSQL | not installed | postgresql@17 17.11 bottled, postgresql@18 18.6 no bottle on this macOS (source build), no postgresql@19 formula; GRAPH_TABLE is in no released PostgreSQL through 18 |
| Python | 3.14.7 | Homebrew |

## Findings

- DuckPGQ (f386a6c on DuckDB 1.5.4): `-[e:chain WHERE e.field IN (...)]->{1,}` is a Binder Error
  (`Referenced table "e" not found`) under `ANY SHORTEST` and under a bounded `{1,5}`; the same edge WHERE works on
  a single hop. Label alternation `[:child_function|child_value]` is a Parser Error in every spelling tried
  (`|`, `IS a|b`, `:a|:b`). Unbounded `->{1,}` without `ANY SHORTEST` is a Constraint Error (WALK mode). The lab
  bakes filters into edge tables (`chain_edge`, `calls_fast`, `calls_slow` with `extern = 0`), one label each.
- DuckPGQ path search pairs every source vertex with every destination vertex before the BFS; a vertex WHERE inside
  the node pattern (`(a:fn WHERE a.id IN (SELECT ...))`) narrows sources and keeps T3/T4 fast, a MATCH-level
  `WHERE o.file = l.file` does not narrow the pairs. T1 (every call to every call) did not finish.
- `LOAD duckpgq` on a `-readonly` database fails (it writes `__duckpgq_internal`); property graphs persist in the file.
- threads=1 vs default: DuckDB T1 recursive CTE 0.51 vs 0.16 s (crates); the plain-UNION T4 walk 3.43 vs 12.88 s
  (parallel default slower); DuckPGQ T3/T4 identical within 0.01 s. `USING KEY` + `recurring` turns T4 from a
  depth-bounded walk (3.4 s) into a BFS (0.02 s) with identical rows.
- SQLite needs `ANALYZE` for the recursive step to use the `(file, parent, sib)` index (118 s -> 0.35 s).
- grit 0.1.1: recursive patterns run per file; `bubble`, `within` + recursion, and list accumulation crash with a
  stack overflow or hang; one crashing file ends the whole run.

## Notation (T1 and T3)

| arm | T1 | T3 |
| --- | --- | --- |
| R1/R2 | `5_recursive_cte.sql` `@T1`, 18 lines | `@T3`, 7 lines (`@T3_key` 8) |
| P1 | `3_duckpgq.sql` `@T1`, 11 lines + graph DDL | `@T3`, 6 lines + graph DDL |
| G | `6_gritql/chain.grit`, 11 lines (link projection) | not_expressible |
| B | `2_chain_depth<k>.scm`, 2 lines per depth | n/a |
| S | `7_sprefa/chain.dl7` sketch | `7_sprefa/reach.dl7` sketch |
