# Research status

Queue authorized 2026-09-07. Coordinator ledger.

The sequential Luna routing was superseded by the user's instruction to finish
D through synthesis in one native Sol pass. `/root/sol_finish_research` owns
that pass. The interrupted D CLI process produced no `4_sql_buffer.md`; D was
therefore written from the saved brief rather than resumed from a partial file.

| Shot | Report | Status | Agent | Dependency | Started | Finished |
|---|---|---|---|---|---|---|
| A | `1_collision.md` | completed | `/root/shot_a_collision` (canonical root launch) | shared brief | 2026-09-07 15:46 EDT | 2026-09-07 16:27 EDT |
| B | `2_animation.md` | completed | `codex-exec/01a07d8e-e86b-7d00-8d96-1dd65bc7ad6f` (resumed session 60903; prior 66428 interrupted after patch error) | shared brief | 2026-09-07 16:28 EDT | 2026-09-07 16:50 EDT |
| C | `3_rollback.md` | completed | `codex-exec/01a07da3-2e02-79f2-a9ca-707295562660` (session 56363) | shared brief | 2026-09-07 16:50 EDT | 2026-09-07 17:00 EDT |
| D | `4_sql_buffer.md` | completed | `/root/sol_finish_research` (supersedes interrupted `codex-exec/01a07da7-c4f1-7831-9571-c74ca02e00fa`) | shared brief | 2026-09-07 17:00 EDT | completed in superseding pass |
| E | `5_hosts.md` | completed | `/root/sol_finish_research` | A, B | superseding pass | completed in superseding pass |
| F | `6_compatibility.md` | completed | `/root/sol_finish_research` | A, B, C | superseding pass | completed in superseding pass |
| G | `7_replay_audit.md` | completed | `/root/sol_finish_research` | A, B, C, F | superseding pass | completed in superseding pass |
| H | `8_storage_audit.md` | completed | `/root/sol_finish_research` | D | superseding pass | completed in superseding pass |
| Synthesis | `9_synthesis.md` | completed | `/root/sol_finish_research` | A-H | superseding pass | completed in superseding pass |

No implementation, install, commit, or experiment was performed. Reports are
confined to this directory.
