# ryi bug report 2026-10-28: five fixes

Base: `origin/main` f3c3a625, worktree `~/projects/hafley-rs-wt/ryi-report-20261028`,
branch `fix/ryi-report-20261028`. Gate: `cd crates/sprefa-extract && cargo test --features cli,read --test all <filter>`,
scoped by filter only, never the full sweep.

## Speech-to-text term map

| heard | code |
| --- | --- |
| Halfly RS / RYI | `hafley-rs` / `ryi` (sprefa-extract bin `ryii`) |
| Crates FreeFoot Extract, features Cle readan, test all | `crates/sprefa-extract`, `--features cli,read`, `--test all` |
| Halfly SEMSRC read lang | `crates/hafley_scm/src/read/lang/` |
| TS Resolve RS | `hafley_scm/src/read/lang/ts_resolve.rs` |
| ECMA 262162161 Step 10 | ECMA-262 16.2.1.6.1 ParseModule step 10 |
| RYIRS, ops query query run to a | `src/bin/ryi.rs`, `ops.rs`, `0_query.rs::run_to` |
| add deadline within or cancel | `src/bin/ryi/0b_deadline.rs::within` |
| 1 SCMPP Rose RS | `src/bin/ryi/1a_scmpp_rows.rs` |
| generator template | `~/projects/hafley-tsp/packages/rust/src/emitter/templates/server_auto.rs` + `07_daemon-files.tsx` |
| compile rs | `hafley_scm/src/scmpp/_2_compile.rs` |
| SCMPP node sig | scm++ node `sib` column |
| GNOME / g 2 | `ryi` / ? (unresolved; see item 1 leftover) |

## Items

1. Default export seeds. `ts_module_facts_from_parsed` maps local export entries onto
   root-scope bindings via `oxc_semantic` `Scoping` (`get_root_binding` -> `symbol_span`).
   Exported local name bound by an import -> indirect export (16.2.1.6.1 step 10).
   `export default wrap(X)` -> `default` seeded at X. Fixture test: plain default, wrapped
   default, re-exported import, named export list. Leftover: `export const x = memo(y)`
   has no def node.
2. Daemon stops on client disconnect. Per-request cancel flag in an op guard owned by the
   handler future and then the response body; drop of either sets it. Flag threads
   `ops::query` -> `query::run_to` -> `deadline::within_or_cancel`; timer checks deadline
   or cancel every 100 ms, calls SQLite interrupt; exit 3 `query: cancelled`.
   `1a_scmpp_rows.rs`: `matches_with_options` + progress callback breaks on stop.
   Same change in generator template; every streaming verb, graph and stratify included.
3. Timeout inside matching: same progress callback. Test: 100-key object literal,
   3-sibling pattern, `--timeout 3` exits 3 in about 4 s.
4. Three unanchored siblings in one flat pattern: compile-time warning in `_2_compile.rs`;
   if feasible lower each sibling to a parent edge plus sibling order condition on `sib`.
   No split without the order condition.
5. `--lang typescript` names the TypeScript family; grammar per file extension (`.tsx` -> tsx).

Optional: store writes captured nodes plus ancestors that field relations need, instead of
every CST node; `@__root`-style program capture keeps text for relations, drops it from output.

Report only: T195 scm++ growth order-dependent failure (compile counts one `Query::new` per level).

## Round 2 (user-set 2026-10-28: "i want it all")

Scale measured before round 2 (median of 3, exponent = log2 of the 40 -> 80 keys ratio):
split scm++ 2.27 (0.62s), unsplit scm++ 4.31 (6.89s), plain `--query` 4.51 (4.27s).

1. N^5 everywhere
   - cross-child `#eq?`/`#not-eq?`/`#match?` on a split level -> SQL conds over child captures (new cond form
     addressing captures by pattern id), so `#not-eq? @a @c` no longer blocks the split
   - 3+ unanchored siblings nested below a level root: lift the nested node into its own level
     (`has? stopBy: neighbor`) and split it there
   - plain `query --query`: a pattern whose node has 3+ unanchored siblings runs through the scm++ split and prints
     the same rows as tree-sitter
2. `export const x = memo(y)`: x gets a def node; its initializer's calls are x's edges (option A).
3. scm++ store: node rows for captured nodes plus the nodes each relation walk reads; program-capture text dropped
   from output rows, kept for relations.
4. T195 order dependence + the 8 tests red on origin/main f3c3a625.
5. Bench, hafley-observe: deterministic tree-sitter work counts (progress-callback ticks) asserted as a growth class,
   plus wall-time median of 3 runs per size with a fitted-exponent ceiling. Both live in hafley-observe.
