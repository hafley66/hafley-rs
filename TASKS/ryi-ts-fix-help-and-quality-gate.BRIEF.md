# ryi TS fix: captured help golden and repository quality gate

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
1. t_178_ryi_help::generated_clap_help_matches_captured_main (tests/178_ryi_help.rs:60): "graph help bytes" differ. The merged lanes changed graph help text
   (PATH@START:END, END exclusive from plan row D25; FILE#render usage from D16). Find the capture source the test compares against and how it is produced.
   If the capture is produced by a command or script, regenerate it with that command; never hand-edit a captured file. If no producer exists, report it and stop.
2. t_186_quality_gate::repository_quality_gate_matches_its_allowlists (tests/186_quality_gate.rs:142). New findings:
   - crates/sprefa-extract/src/edit/_3_stage.rs:26 env_read_in_request_path (std::env::var in an edit function)
   - crates/hafley_scm/src/read/lang/7c_rust_core_iterator_methods.rs:26, crates/sprefa-extract/src/edit/1e_ts7_graph_target.rs:22,
     crates/sprefa-extract/src/edit/1g_ts7_resolve.rs:34 free_fn_name_in_three_files
   (read the full list from the test; there may be more). Fix the code: pass env values in from the CLI boundary; give or share the duplicated
   free function so the name is defined once. Do not add allowlist entries.
