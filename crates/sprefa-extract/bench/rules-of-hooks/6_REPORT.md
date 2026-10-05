# Rules of hooks: pre-ryiii, 2026-10-05

The worktree build and both extraction passes completed over all 114 non-Flow
fixtures. The original upstream suite has 118 cases: 50 valid and 68 invalid;
4 Flow cases are retained and excluded from this run. Reference messages come
from React commit `ae74234eae6ebd62f19190731278e20bc1c37d51` (`v19.2.0`).

## Per-rule results: pre-ryiii

| rule | case_count | true_positive | false_positive | false_negative |
| --- | ---: | ---: | ---: | ---: |
| async | 114 | 0 | 0 | 6 |
| caller | 114 | 22 | 3 | 9 |
| class | 114 | 0 | 0 | 10 |
| conditional | 114 | 11 | 2 | 5 |
| early_return | 114 | 3 | 7 | 0 |
| effect_event | 114 | 0 | 0 | 12 |
| loop | 114 | 15 | 0 | 0 |
| nested_callback | 114 | 4 | 20 | 0 |
| try_catch | 114 | 0 | 0 | 2 |

The five implemented rules are caller, loop, conditional, early_return, and
nested_callback. Async, class, try_catch, and effect_event rows retain additional
reference behaviors. Bare `use` messages remain in their reference category;
that name is excluded by the brief's `^use[A-Z0-9]` predicate.

`case_count` is 114 for each rule. Positive/negative counts are messages matched
as a multiset by `(fixture path, rule, hook name)`, with actual calls deduplicated
by span. Thus cases with repeated calls contribute multiple messages. The
upstream declarations do not give expected source locations. A matching count
cannot establish that the exact source calls match. Gap rows produce no
violation; unmatched expected messages remain false negatives.

Across all reported behaviors: 55 true positives, 32 false positives, and
44 false negatives. [Raw table](9_SCORE.json) is the unchanged output of the
single scoring query in `8_score.sql`.

## Every disagreement and missing fact

There are 66 unequal `(path, rule, hook)` entries across 52 files: 29 false-positive
entries and 37 false-negative entries. Counts within each entry preserve
multiplicity. Every entry has a file and reason in
[the filterable disagreement grid](11_DISAGREEMENTS.html) and
[the complete JSON list](10_DISAGREEMENTS.json). Reasons are computed by SQL from
findings, gaps, captures, and the reference behavior category.

There are 14 missing-owner rows across 12 files. Each lacks a matching
`df.call_res` row for the full hook-call span. [Gap rows](12_GAPS.json) retain
file, byte span, hook, and reason; [the gap grid](14_GAPS.html) displays them.
These affect nested declarations, labeled statements, class-property initializers,
and returned/export-default arrows in the vendored cases. No Rust fact workaround
was added. There are no `missing_site` or parse-error rows in the final run.

Observed disagreements include anonymous memo/forwardRef render functions being
classified as callbacks, lowercase namespace members matching the terminal-name
predicate, class methods being treated according to owner naming, and extra
return-order findings inside loops. `fixtures/valid/010_Case.tsx` contains an
unreachable hook after an unconditional return: SQL reports early_return and the
reference accepts it. Span order does not encode reachability or rule priority.

## Run evidence and checks

- Built in this worktree with
  `KACHE_DISABLED=1 cargo build --features cli,ts-checker --bin ryii`.
  Build passed in 10m 46s. Build stamp: `c2299b190d87`, `2026-10-05T23:15:06Z`.
- Used the lane's `$CARGO_TARGET_DIR/debug/ryii` for every new extraction and
  adapter check. No installed `~/.cargo/bin/ryii` invocation occurred in this run.
- Both passes used the same 114-path list. Database path inventories match it.
  Fast extraction wrote 2,990 rows, including 13 `df_loop` and 20 `df_nest` rows.
  The query store reported 8,573 written rows and materialized 182 hook captures.
- The query database contains all 118 case metadata rows, 99 non-Flow expected
  messages, and 101 actual rows: 87 violations and 14 gaps.
- SQL imported `1_cases.json` directly with `readfile` and `json_each` into the
  query database. The `pre-ryiii` exception was explicitly authorized for this run;
  no separate scoring script or comparison runner was created.
- Fixture regeneration is byte-stable. Both JavaScript files pass `node --check`.
  The single-case adapter completes on invalid-024 with the worktree binary.
  SQL count checks confirm that duplicate findings at one span count once.
- `scripts/bench_grid.py --json` renders the SQL-exported disagreement and gap
  rows without scoring them. The disagreement grid was opened for review.
- Both earlier probe `.db` files were deleted. Runtime databases remain ignored
  under `runs/`. No Rust source changed. No merge or push was performed.

[Run provenance](13_RUN.json) records the binary SHA-256, source HEAD, program
hashes, input-list hash, commands, and row counts. Reproduction commands are in
[the README](5_README.md). `ryiii` registration remains pending its implementation.
