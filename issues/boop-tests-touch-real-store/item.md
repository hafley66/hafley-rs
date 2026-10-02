---
created: 2026-10-01
updated: 2026-10-01
type: bug
status: open
priority: high
epic: burndown-2026-10
labels: [boop]
---

# boop tests open the real ~/.agent/boop.db (migrated the live store to schema 40)

## Description

2026-10-01 21:00: coordinator ran `cargo test -p boop-proc --lib` and `cargo test -p boop --bin boop` on the schema-40 build in merge/main; ~/.agent/boop.db changed 21:01-21:03 and is now user_version 40 (data intact: favorites 306, tags 418). Some unit test resolves the store via the default path (HOME/~/.agent) instead of a temp dir. Find it (run each test with HOME pointed at an empty dir and a read-only sentinel at the real path), make every test hermetic, and add a guard that panics in cfg(test) if the store path resolves under the real HOME. Installed binary now: local main 6fa8810a release, guard skipped by user choice; old binary at ~/backups/boop/boop-bin-0.0.10-248dfdd3.
