---
created: 2026-08-24
updated: 2026-08-24
type: improvement
status: open
priority: high
epic: boop-one-path
labels: [domain-boop]
size: L
---

# One sqlite mailbox replaces bus.ndjson + registry.json

## Description

## Description

Rows live in `~/.agent/mail/bus.ndjson`, routes in `registry.json`, delivery receipts in sqlite `agent_delivery` / `agent_delivery_transition`. `boop debug` joins them and old rows print `no delivery transition`; the reconciler in `crates/boop-proc/src/deliver.rs` exists only to bridge the two stores.

Cut: one sqlite table per row with its transition list; append and first transition in one insert; `--mail-dir` becomes `--db`. Delete the reconciler.

## Acceptance Criteria

- [ ] `bus.ndjson` and `registry.json` no longer written
- [ ] a row cannot exist without a transition (schema constraint + test)
- [ ] `boop debug <lane>` section 2 never prints `no delivery transition`
- [ ] migration reads an existing ndjson once
