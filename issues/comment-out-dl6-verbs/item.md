---
created: 2026-08-24
updated: 2026-08-24
type: improvement
status: open
priority: high
epic: boop-one-path
labels: [domain-boop]
size: S
---

# Comment out concatmap and host

## Description

## Description

`concatmap` and `host` are the DL6 runtime inside the agent-bus binary, 2 of 15 top-level verbs. User call 2026-08-25: comment them out for now, no move to another binary yet.

## Acceptance Criteria

- [ ] both variants and their `run_*` wiring under `#[cfg(feature = "dl6")]`, feature off by default
- [ ] `boop --help` no longer lists them
- [ ] `cargo build --features dl6` still compiles
