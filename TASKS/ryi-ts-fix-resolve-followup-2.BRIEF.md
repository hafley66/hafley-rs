# ryi TS fix: resolve output follow-ups after the resolve regression merge

Repo hafley-rs, crate crates/sprefa-extract. Base: integrate/ryi-ts. Prior lane: feature/ryi-ts-fix-resolve-regression (011bfc60, 62fd3c09, edbd2b74; REPORT.md section from 24f51bd2).

## Failing
- t_134_ts_binding_legs::a_const_binding_shadow_kills_the_name_match (134:207): expected [("project","inferred")], got [].
- t_134_ts_binding_legs::a_self_named_initializer_still_binds_the_outer_fn (134:239): got [("constCase","project",".../ts_binding_legs/shadow.ts","same_file")].
- t_136_untyped_receiver_ts::free_call_keeps_name_match (136:146): 3 vs 2; rows [("mk","use.ts","same_file") x3, ("push","defs.ts","receiver"), ("push","use.ts","same_file")].
- t_136_untyped_receiver_ts::untyped_receiver_member_call_drops_inferred (136:118): 2 vs 3.
- t_193_ts_rtkq_jsx::written_calls_and_nested_jsx_match_jsonl_and_sqlite_goldens (193:73): --resolve JSONL differs from golden.
- t_193_ts_rtkq_jsx::written_tsx_calls_are_additive_to_the_original_resolved_site (193:132): same rows, order differs: got call_site before resolved_edge; golden has resolved_edge first.
- dogfood/ts/J01.sh: `ryii slow --root <fixture> --sqlite db <fixture>` writes 0 jsx_element and 0 jsx_attribute rows (--resolve --ts-checker writes 3 and 2). J01 asserts both tiers carry them.

## User decision 2026-10-02 (round 2)
Prior round stopped on a contradiction (REPORT.md section from 50b0b88e). Plan row D9 wins over test 134: a same-file callable const
(`const project = () => {}`) resolves to its local definition. Update the test 134 assertions that encode the old unresolved/inferred
behavior for callable consts to the D9 edge; keep its prohibition on an edge into free.ts. Then fix the remaining items in the list above
(self-named initializer, 136, 193 golden + row order, slow-tier JSX rows for J01) under the same rule: a plan row wins over an older test;
update the older test's assertion and name the row in REPORT.md.

Rules:
- CODE COMPLETE ONLY. Do not run cargo, npm, node, or dogfood scripts. The coordinator runs every gate, one at a time.
- These tests pass on main b3673b84 and fail on integrate/ryi-ts. Read the test, `git show main:<path>`, and `git log main..HEAD -- <path>` to find the commit that changed behavior.
- Do not edit a test assertion or golden unless the new output is a deliberate change named by a plan row in plans/2026-10-01-ryi-ts-utility.md; if so, name the row in REPORT.md. If a plan row and an existing test contradict, write the contradiction in REPORT.md and stop.
- ryi emits facts only. No SQL analyses, no new CLI commands or flags.
- Other lanes edit the same crate in parallel. Keep diffs inside your area. No drive-by renames or formatting. No push. No Python. No `boop beep scream`. No hand-written files labelled generated.
- Commit per cause; messages end `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`.
- On done: REPORT.md section per test: cause (file:line, commit), change, cargo test filter for the coordinator (target `all`, crate crates/sprefa-extract).
