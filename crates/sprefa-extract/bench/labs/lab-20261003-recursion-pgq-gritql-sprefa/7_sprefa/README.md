# Arm S: sprefa rules over the recursion lab's exports

Lab: `crates/sprefa-extract/bench/labs/lab-20261003-recursion-pgq-gritql-sprefa/` in the worktree
`/Users/chrishafley/projects/hafley-rs/.boop-worktrees/lab/recursion` (branch `lab/20261003-recursion-pgq-gritql-sprefa`).
`LAB` below is that absolute directory. Inputs are gitignored under `LAB/db/`; `LAB/0_export.sh` and
`LAB/1_oracle.py` rebuild them.

## Inputs (corpus = `small` | `crates`)

| file | content |
| --- | --- |
| `LAB/db/lab-<corpus>.db` | SQLite, every table below |
| `LAB/db/lab-<corpus>.duckdb` | DuckDB, same tables plus `cst_vertex`, `chain_edge`, `calls_fast`, `calls_slow` |
| `LAB/db/parquet-<corpus>/<table>.parquet` | Parquet copy of every DuckDB table (`EXPORT DATABASE`), plus `t4_pair.parquet` |

| table | columns | note |
| --- | --- | --- |
| `scmpp_node` | `file, pre, last, parent, depth, sib, idx, kind, field, start, end, named` | every CST node; `parent = -1` at the root; `field = 0` when the node has no field |
| `scmpp_dict_kind`, `scmpp_dict_field`, `scmpp_dict_path`, `scmpp_dict_text`, `scmpp_dict_capture` | `id, text` | interned strings |
| `scmpp_capture` | `file, pattern, match, capture, node, start, end, text` | one row per identifier node (`pattern = 0`); `text` -> `scmpp_dict_text` (T2 needs it) |
| `fn` | `id, path_id, name_id` | function identity is (path, name) |
| `fn_dict_path`, `fn_dict_name` | `id, text` | |
| `call_edge_fast`, `call_edge_slow` | `src_fn_id, dst_fn_id, extern` | fast = `ryii --resolve`, slow = `ryii graph --slow --sqlite`; `extern = 1` into the std/core shim files |
| `seed` | `anchor, fn_id` | 12 seeds |
| `t4_pair` | `tier, anchor, seed_fn_id, target_fn_id` | one target per seed and tier |

## Tasks and expected output

Rule sketches: `chain.dl7` (T1), `reach.dl7` (T3), `shortest.dl7` (T4). T2 is functions whose body calls
themselves by bare identifier: `function_item` F, its `name` child identifier text N, a `call_expression`
descendant of F's `body` child whose `function` child is an `identifier` with text N.

Write one TSV per run to `LAB/7_sprefa/out/<output>.tsv` with a header row, tab separated, no quoting:

| task | tier | columns |
| --- | --- | --- |
| T1 | cst | `path, outer_start, outer_end, link_start, link_end` |
| T2 | cst | `path, fn_start, fn_end` (the `function_item` byte span) |
| T3 | fast, slow | `anchor, path, name` |
| T4 | fast, slow | `anchor, target_path, target_name, length` |

Append one row per run to `LAB/7_sprefa/runs.tsv` (header first):

`arm task corpus tier threads wall_s peak_rss_mb setup_s notation_lines status note output`

with `arm = S`, `status` in `ok | not_expressible | not_available | error`, and `output` = the TSV name without
`.tsv`, e.g. `S_T3_crates_fast`. `LAB/8_compare.py` scores them against `LAB/db/oracle/`.
