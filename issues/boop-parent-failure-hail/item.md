---
created: 2026-08-18
updated: 2026-08-19
type: feature
status: closed
priority: high
related: ['@boop-tell-parent', '@boop-debug-recent-errors']
labels:
- area:boop
---

# Boop hails parent when a lane degrades or fails

## Description

The lane supervisor sends typed parent mail for actionable state transitions. Notify once when provider retries begin, once when the retry budget is exhausted, and once when a lane exits without a completion result. Include lane, harness, model, attempt count, reason, last provider finish/error fields, and the command for diagnostics. Derive the parent from the registered edge. Deduplicate repeated identical warnings and avoid per-poll mail. Delivery failure remains recorded in the mailbox. Add deterministic supervisor tests for recovery, exhaustion, missing completion, and parentless lanes.

## Landed

Three typed rows reach the parent, each at most once per lane: `retrying`,
`retry_budget_exhausted`, and `exited_without_completion`. Dedup is against
the mailbox itself, so a respawned supervisor sends nothing it already sent.
Covered by `each_failure_kind_reaches_the_parent_exactly_once` in
`crates/boop/tests/parent_failure_hail.rs`.
