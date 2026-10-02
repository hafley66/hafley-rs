# ryi TS fix: --resolve stopped emitting resolved_edge on TypeScript

Repo hafley-rs, crate crates/sprefa-extract (its own workspace root; tests are one target `all`). Base: branch integrate/ryi-ts (6 earlier ryi TS lanes merged).
These tests pass on main b3673b84 and fail on integrate/ryi-ts. Restore them without undoing the TS facts the merged lanes added.

Rules:
- CODE COMPLETE ONLY. Do not run cargo build, cargo test, cargo check, npm, node, or dogfood scripts. The coordinator runs every gate, one at a time. Machine load.
- Read the failing test, the main version of the code (git show main:<path>), and the merged lane diff (git log main..HEAD -- <path>) to find the cause.
- Do not edit a test assertion or golden to match new output unless the new output is a deliberate fact change listed in that lane's plan row; if so, say which row in REPORT.md.
- ryi emits facts only. No SQL analyses, no new CLI commands or flags.
- Other lanes edit the same crate in parallel. Keep diffs inside your area. No drive-by renames or formatting.
- No hand-written files labelled generated. No `boop beep scream`. No push. No Python.
- Commit per cause; messages end `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`.
- On done: REPORT.md section per test: cause (file:line, commit that introduced it), change, and the cargo test filter the coordinator should run.

## Failing tests
t_1_resolve_cli::resolve_mode_streams_cross_file_edges, t_1_resolve_cli::resolve_type_arm_streams_resolved_type_edges,
t_1a_resolve_raw::raw_sink_gets_file_and_syntax_facts_from_the_resolve_inputs,
t_23_flow_cli_dispatch::{call_and_flow_arms_emit_both_families, flow_is_a_resolve_arm, resolve_without_family_is_byte_identical},
t_4_capability_parity::every_library_capability_is_reachable_through_the_binary, t_55_diff_verb::{one_to_two_matches_the_hand_derived_rows, a_dirty_worktree_does_not_change_the_delta},
t_8_scip_families_cli::the_diet_scip_family_stream_is_the_fast_output, t_98_resolve_witness::{one_witness_per_leg_on_a_syntax_run, the_flag_off_stream_is_the_committed_golden},
t_167_graph_paths::flow_paths_follow_derived_interprocedural_edges ("fixture has a flow edge"), t_168_graph_revision::a_path_added_between_commits_is_reported_once (0 vs 1),
t_golden_parity::call_resolve_scip_ratchet_ts ("ts/corpus_unique: true 6 below the pinned floor 8").
## Observed
`ryii --resolve tests/fixtures/resolve/0_caller.ts tests/fixtures/resolve/1_callee.ts` now prints only
{"record":"call_site","callee":"helper",...,"start":33,"end":41}; main printed {"record":"resolved_edge",...,"caller_site_start":33,"caller_site_end":39,...}.
Note the call span changed too: 33..39 (callee name) on main vs 33..41 (whole call) now. Raw sink now gets symbol rows where main had none.
Suspect: 826a1a05 "fix(ryii): emit written TS calls and nested JSX facts" (crates/hafley_scm/src/read/lang/7_scm_rows.rs, ts.rs, project.rs, types.rs).
## Task
resolved_edge, flow_edge and the scip family stream come back byte-identical to main for these fixtures. The new call_site / jsx_element / jsx_attribute
records stay in fast output; decide from plans/2026-10-01-ryi-ts-utility.md and TASKS/ryi-ts-rtkq-jsx-golden.BRIEF.md whether they belong in resolve output,
and if they do, they must be additive and must not displace or reshape resolved_edge.
