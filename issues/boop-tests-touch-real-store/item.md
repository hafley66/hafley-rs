---
created: 2026-10-01
updated: 2026-10-02
type: bug
status: fixed
priority: high
epic: burndown-2026-10
labels: [boop]
closed: 2026-10-02
closed_by: claude-375
---

# boop tests open the real ~/.agent/boop.db (migrated the live store to schema 40)

## Description

2026-10-01 21:00: coordinator ran `cargo test -p boop-proc --lib` and `cargo test -p boop --bin boop` on the schema-40 build in merge/main; ~/.agent/boop.db changed 21:01-21:03 and is now user_version 40 (data intact: favorites 306, tags 418). Some unit test resolves the store via the default path (HOME/~/.agent) instead of a temp dir. Find it (run each test with HOME pointed at an empty dir and a read-only sentinel at the real path), make every test hermetic, and add a guard that panics in cfg(test) if the store path resolves under the real HOME. Installed binary now: local main 6fa8810a release, guard skipped by user choice; old binary at ~/backups/boop/boop-bin-0.0.10-248dfdd3.

## Comments

### 2026-10-02T13:51:19Z · @claude-375

Merged to main 2d666ded (09896805 + 9f34a927 harness filter + 2d666ded _N_ modules). Gates sandboxed HOME: 22 binaries 1115 pass / 9 fail (1 pre-existing harness_boundaries on 1_user_slice.rs:26; 8 env e2e tui_revive/worktree_reclaim/omp_live, not run on main baseline). Contract 82 ok 0 not-ok. Installed ~/.cargo/bin/boop from this tree; sync budget error stopped 13:45:51.
