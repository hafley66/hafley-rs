---
created: 2026-10-01
updated: 2026-10-02
type: bug
status: fixed
priority: normal
epic: burndown-2026-10
labels: [boop]
closed: 2026-10-02
closed_by: claude-375
---

# transcript sync aborts: trace join observation budget exceeded 96821 > 10000

## Description

After the live store reached schema 40, every sync logs: Error: trace join observation budget exceeded: 96821 > 10000 (crates/boop-store/src/0_trace_identity.rs:109, from 7c9b54bf). Commands still run (tag recent OK), but projection aborts. Check whether schema 40 changed what the join reads.

## Comments

### 2026-10-02T13:51:19Z · @claude-375

Merged to main 2d666ded (09896805 + 9f34a927 harness filter + 2d666ded _N_ modules). Gates sandboxed HOME: 22 binaries 1115 pass / 9 fail (1 pre-existing harness_boundaries on 1_user_slice.rs:26; 8 env e2e tui_revive/worktree_reclaim/omp_live, not run on main baseline). Contract 82 ok 0 not-ok. Installed ~/.cargo/bin/boop from this tree; sync budget error stopped 13:45:51.
