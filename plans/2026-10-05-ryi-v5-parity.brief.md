# Brief: ryi in-repo facts sprefa v5 asserted

Read first: /Users/chrishafley/projects/hafley-rs/.boop-worktrees/merge/ryi-coordinator/plans/2026-10-05-ryi-v5-parity.plan.md
(table of items, v5 test lines, ryi sites). v5 source: /Users/chrishafley/projects/sprefa/v5 (read-only).
Repo rules: hafley-rs CLAUDE.md; skill ~/projects/claude-research/skills/hafley-rs-repo/SKILL.md.

## Work, in this order, one commit each
1. Labeled break: `break 'outer produce()` gets a flow edge into the labeled loop and on to the
   binding (v5 dataflow.rs:1116). Code: hafley_scm/src/lang/rust/11_df_syntax_rows.rs:648, :742.
   The loop label is a plain CST child, not field "label"; confirm with the CST first.
2. End line on call nodes in JSONL output (`--lines` today gives start line only).
3. Owning function on each flow node: column naming the enclosing function, class-qualified for
   methods (`Widget.render`). Same column name in JSONL and sqlite.
4. JSX: element becomes a `new` node with one `df_field` per attribute, attribute expressions
   lifted through the existing expression lifts (ts.rs:3603/3713/3761/3772/3793); attribute value
   flows to the component's destructured param or `props.<name>` member read. Site: ts.rs:3824.
   Fixture: port the three `src/app.tsx` cases from v5 tests/it/flow_jsx.rs.
5. Transitive reach within one repo: one command answering "what does X reach" / "what reaches X"
   over df edges inside bodies plus `flow_edge` across functions. Extend `graph --flow-path` rather
   than adding a second walker (one implementation per concern). Demand-driven: walk from the asked
   node only. Port v5 dataflow.rs reach cases (:140 :239 :341 :925 :1029 :1116) and flow_jsx.rs:137
   (10 checks incl. guarded `&&` must not reach) as tests.
6. Read/write call classification (v5 `call_kind`, src/storage/call.rs:1057, :1125): report only.
   Find where v5's classification comes from (rule file, table, heuristic), list its inputs and
   outputs, and propose where it sits in ryi's df rows. No code.

## Limits
- Fast tier only (no type checking) unless an item proves it needs types; stop and report then.
- Tests: toMatchInlineSnapshot-style whole-output snapshots (insta) over granular asserts; no
  per-test duplicated setup. New code in new small numbered files; do not grow files over ~1000 lines.
- Commands under 2 min; no whole-corpus runs; build with KACHE_DISABLED=1.
- Gate before each commit: targeted tests for the touched area; before the last commit:
  `cargo test --features cli --no-fail-fast` in crates/sprefa-extract. Known pre-existing failures:
  t_98 x2, t_101, t_92 x2, flaky t_194.
- No merge, no push.

## Report
Per item: commit hash, file:line of the change, test names, before/after rows on the ported v5 case.
Exact error text for anything that failed or was stopped.
