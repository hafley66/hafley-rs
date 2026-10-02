# ryi TS fix: slow tier decline diagnostics vs the missing-checker refusal

Repo hafley-rs, crate crates/sprefa-extract. Base: integrate/ryi-ts. Suspect: feature/ryi-ts-slow D15 (71669f2f "refuse slow TypeScript when ts-checker is missing").

## Failing
t_104_tier_decline_diagnostic::{no_witness_emits_no_diagnostic, the_declined_stream_survives_the_reverse_door, ts_tier_off_path_is_a_diagnostic} (104:41):
`ryi slow: TypeScript requires cargo feature ts-checker`. The suite runs with `--features cli` (no ts-checker). Plan row D15: missing checker gives a non-zero
exit naming the feature, never 0 edges with exit 0. Find what inputs test 104 feeds slow and whether they are TypeScript requests that need the checker.
Narrow the refusal to the case D15 names, or report the contradiction and stop.

Rules:
- CODE COMPLETE ONLY. Do not run cargo, npm, node, or dogfood scripts. The coordinator runs every gate, one at a time.
- These tests pass on main b3673b84 and fail on integrate/ryi-ts. Read the test, `git show main:<path>`, and `git log main..HEAD -- <path>` to find the commit that changed behavior.
- Do not edit a test assertion or golden unless the new output is a deliberate change named by a plan row in plans/2026-10-01-ryi-ts-utility.md; if so, name the row in REPORT.md. If a plan row and an existing test contradict, write the contradiction in REPORT.md and stop.
- ryi emits facts only. No SQL analyses, no new CLI commands or flags.
- Other lanes edit the same crate in parallel. Keep diffs inside your area. No drive-by renames or formatting. No push. No Python. No `boop beep scream`. No hand-written files labelled generated.
- Commit per cause; messages end `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`.
- On done: REPORT.md section per test: cause (file:line, commit), change, cargo test filter for the coordinator (target `all`, crate crates/sprefa-extract).
