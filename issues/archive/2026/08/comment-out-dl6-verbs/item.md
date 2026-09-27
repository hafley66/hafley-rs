---
created: 2026-08-24
updated: 2026-08-25
type: improvement
status: done
priority: high
epic: boop-one-path
labels: [domain-boop]
size: S
closed: 2026-08-25
---

# Comment out concatmap and host

## Description

## Description

`concatmap` and `host` are the DL6 runtime inside the agent-bus binary, 2 of 15 top-level verbs. User call 2026-08-25: comment them out for now, no move to another binary yet.

## Acceptance Criteria

- [x] both variants and their `run_*` wiring under `#[cfg(feature = "dl6")]`, feature off by default
- [x] `boop --help` no longer lists them
- [x] `cargo build --features dl6` still compiles

## Agent Runs

### 2026-08-25T04:18:37Z · @chore-verb-cuts

d71b2cc concatmap and host clap variants plus their run_* call sites cfg-gated behind feature dl6 (off by default); Cargo.toml required-features=[dl6] added for tests/host_chat.rs and tests/concatmap_e2e.rs; cargo build -p boop and cargo build -p boop --features dl6 both compile.
