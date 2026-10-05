# ryi: in-repo facts sprefa v5 asserted (2026-10-05)

Ownership: ryi answers about one repo at one commit; sprefa joins across repos and
commits. Survey source: sprefa/v5 tests (flow_jsx.rs, dataflow.rs, call_golden.rs).

## In ryi (this plan)

| # | fact | v5 check | ryi site today |
|---|---|---|---|
| 1 | transitive reach inside one repo, including inside a function body | dataflow.rs :140 :239 :341 :925 :1029 :1116; flow_jsx.rs :137 | `graph --flow-path` walks only cross-function `flow_edge` |
| 2 | labeled break value flows into its binding | dataflow.rs :1116 | `hafley_scm/src/lang/rust/11_df_syntax_rows.rs:648`, `:742` |
| 3 | owning function on each flow node (`Widget.render`) | dataflow.rs :239, :578 | none; span containment only |
| 4 | end line on call nodes in JSONL | call_golden.rs (call_def) | sqlite `line_start` only |
| 5 | JSX element = `new` node, `df_field` per attribute, attribute expressions lifted; prop flows to component param | flow_jsx.rs :73 :137 :254 | `hafley_scm/src/read/lang/ts.rs:3824` -> one `expr` node |
| 6 | read/write classification of call sites (v5 `call_kind`) | call_golden.rs (call_kind, 2 rows); v5 `src/storage/call.rs:1057`, `:1125` | none; scope to be confirmed after reading v5 source |

## In sprefa (not here)
Per-revision rows (`_rev`), rules + string builtins (RTKQ linking), runner checks
(budget, watchdog 124, verdict, db-ratio).

## Contract
Symbol spelling: ryi's form is canonical; v5 goldens change, nothing renamed.

## Order
2, 4, 3, 5, 1, 6. One commit per item. Item 6 stops after a report of v5's classification source.
