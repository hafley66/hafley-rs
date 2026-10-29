# ryi bug report 2026-10-28: receipts

Branches:
- hafley-rs `fix/ryi-report-20261028` (worktree `~/projects/hafley-rs-wt/ryi-report-20261028`) off `origin/main` f3c3a625
- hafley-tsp `fix/ryi-request-cancel` (worktree `~/projects/hafley-tsp-wt/ryi-request-cancel`) off `emitter-routes` 9cb2235

Gate: `cd crates/sprefa-extract && cargo test --features cli,read --test all -- <filter>`. Red = the test run once with
the fix switched off, then restored.

## Round 1: the five report items

| item | test | green | red |
| --- | --- | --- | --- |
| 1 default export seeds | `ts_default_exports_resolve_to_their_declaration` | pass | 12 rows lost |
| 2 cancel on disconnect | `daemon_query_stops_when_its_client_disconnects` | pass | daemon kept matching: 2.01s CPU in 2s |
| 3 timeout inside matching | `query_timeout_stops_inside_one_match` + `growth_bench_timeout_latency_is_flat` | exit 3, latency flat in n | 18.6s for `--timeout 3` |
| 4 sibling split | `scmpp_sibling_split_answers_tree_sitter_rows` | 14 cases = tree-sitter oracle | no order condition: 125 vs 10 rows |
| 5 tsx under `--lang typescript` | `query_lang_typescript_picks_tsx_by_extension` | pass | snapshot diff |

## Round 2 (user: "i want it all")

| item | change | test | red |
| --- | --- | --- | --- |
| N^5: cross-child `#eq?`/`#not-eq?` | `Cond::PinnedTextEq` over split children | 222 cases 6, 7, 9 | - |
| N^5: nested 3+ siblings | a child holding such a node splits its parent too | 222 cases 8, 9 | - |
| N^5: `(x)+` siblings | `+` without inner capture splits like `(x)` | 222 cases 10-13 (12-arg source) | - |
| N^5: plain `query --query` | splittable patterns run through scm++, `--query` row shape | 222 `query_same_as_tree_sitter` | - |
| SQL join order | mandatory captures inner-join; `scmpp_capture_text` index | `growth_bench_cross_eq_split` | count k 2.97 -> 1.86 |
| `export const x = memo(y)` | `wraps_callable`: x gets a def; edges x->memo, x->y | 220 snapshot | 8 rows lost |
| store trim | in-memory runs write captured nodes + field-ancestor paths only | `scmpp_trimmed_store_answers_the_full_store_rows` (9 walks) | ancestor and sibling reads each flip `same_rows` |
| whole-file text | stdout drops `NAME__text` of a capture on the tree root | `scmpp_stdout_drops_whole_file_capture_text` | - |
| `nth-child` | no `of`: `sib = N-1`; `of`: one ranked CTE | `growth_bench_nth_child_of_is_linear` | k 1.79 before |
| T195 | `hafley_observe::counted`: global `filter_fn(|_| false)` default so no thread caches "never" | 8 runs | old helper: 6 of 8 fail |

## hafley-observe `growth_bench` (median of 3 runs, fitted exponent at the largest pair)

| bench | count k | time k | ceiling count/time |
| --- | --- | --- | --- |
| sibling split (C(n,3) rows) | 2.92 | 2.81 | 3.2 / 3.6 |
| `+` sibling (C(n,3) rows) | 2.92 | 2.81 | 3.2 / 3.6 |
| cross-child `#eq?`, 0 rows | 1.86 | 0.60 | 2.2 / 2.6 |
| rules-of-hooks program | 0.97 | 0.74 | 1.2 / 1.5 |
| `nth-child 3 of` | 0.99 | 0.69 | 1.2 / 1.5 |
| `--timeout 1` stop latency | 0 | 0.00 | 0 / 0.3 |

Count = rows written + SQLite VM steps (`WorkTrace`). Tree-sitter calls the progress callback once per 100 cursor
steps (`query.c:4046`); state blow-up inside one step is invisible to counts, so wall time carries it.

## Perf survey (median of 3, k at the last doubling)

Superlinear before round 2: quantified siblings 4.15, `nth-child of` 1.79 (both fixed above). Binary chain + `has?`
rows: each grows 2.58 because each of n rows carries the n-wide declaration text (2.1 GB stdout at 16000 terms):
output-bound. Linear or below: JSX depth + `has-ancestor?`, block `precedes? stopBy`, many files + hooks, nested
arrows + ancestor field, `not-has?` nesting, `has? stopBy` deep parens, `rows: list`, fast tier on deep generics and
big objects, resolve with default-memo exports.

## Not split (warn only)

`?`/`*` siblings (tree-sitter keeps an absent row only when no candidate sits between the neighbours), `+` with a
capture inside the group, anchored siblings, negated fields, relations or cross-level conditions on the split level.

## Regression

Local build (`RCARGO_OFF=1`), filters `scmpp hooks query growth ts_ v5_parity graph default_seats binding_legs fast_scm
ratchet golden stratify client_daemon cleave_ts quality_gate t_195 t_15_ t_190 t_221`: 291 passed after the bench lock.
hafley_scm `_1_scmpp_compile`: 20 passed (SQL goldens: LEFT JOIN -> JOIN for mandatory captures, nth-child forms).
hafley-observe: all pass. hafley-tsp emitter vitest: 18 passed, 4 skipped; regenerated `server_auto.rs` identical.

The 8 "base reds" were environment:
- `ts7/node_modules` missing in a fresh worktree (`npm ci` in `crates/sprefa-extract/ts7`, TypeScript 7.0.2):
  `t_134`, `t_167`, `t_168`, `t_195_ts_lib_globals`.
- rcargo runs the test binary here with the Spark's compile-time `CARGO`/`RUSTUP_HOME`: `tests/support/0b_host_cargo.rs`
  fixes `t_15` x3 and `t_190` x6 under rcargo. Still rcargo-only red, green locally: `t_172`, `t_155a`, `t_5_scip_facts_cli`,
  `t_golden_parity` (fixture-runner children get the remote `PATH`).
