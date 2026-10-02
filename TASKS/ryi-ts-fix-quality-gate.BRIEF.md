# ryi TS fix: repository quality gate findings from merged lanes

Repo hafley-rs. Base: branch integrate/ryi-ts. Read REPORT.md section "ryi TS help and quality gate repair (2026-10-02)" (commit e2cf9c6c) for traced causes.
Failing: t_186_quality_gate::repository_quality_gate_matches_its_allowlists (delegates to scripts/quality-gate.sh). Help capture (t_178) is out of scope; the coordinator regenerates it.

## Task
- crates/sprefa-extract/src/edit/_3_stage.rs:26 env_read_in_request_path: `requested_state_root` (D22, 29a2c385) reads HOME. Read HOME at the CLI boundary and pass it in. Keep D22 behavior: refuse internal state before creating it.
- free_fn_name_in_three_files `position`: crates/sprefa-extract/src/edit/1e_ts7_graph_target.rs:22, 1g_ts7_resolve.rs:34 (D14, 1fab89cb), crates/hafley_scm/src/read/lang/7c_rust_core_iterator_methods.rs:26.
  Make 1g_ts7_resolve.rs use the one existing byte-offset-to-LSP-position helper in 1e_ts7_graph_target.rs instead of defining its own.
- Read scripts/quality-gate.sh rules and check every file changed in `git diff main...integrate/ryi-ts --stat` against them by reading; fix further findings the same way.
- No allowlist entries.

Rules:
- CODE COMPLETE ONLY. Do not run cargo, npm, node, scripts/quality-gate.sh, or dogfood scripts. The coordinator runs every gate, one at a time.
- Keep diffs to the named findings. No drive-by renames or formatting. No push. No Python. No `boop beep scream`.
- Commit per finding; messages end `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`.
- On done: REPORT.md section per finding: change and file:line.
