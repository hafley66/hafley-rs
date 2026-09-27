---
created: 2026-08-18
updated: 2026-09-26
type: feature
status: obsolete
priority: high
related: ['@boop-tell-parent', '@boop-debug-recent-errors']
labels:
- area:boop
closed: 2026-09-26
closed_by: codex
---

# Boop hails parent when a lane degrades or fails

## Description

The lane supervisor sends typed parent mail for actionable state transitions. Notify once when provider retries begin, once when the retry budget is exhausted, and once when a lane exits without a completion result. Include lane, harness, model, attempt count, reason, last provider finish/error fields, and the command for diagnostics. Derive the parent from the registered edge. Deduplicate repeated identical warnings and avoid per-poll mail. Delivery failure remains recorded in the mailbox. Add deterministic supervisor tests for recovery, exhaustion, missing completion, and parentless lanes.

## Resolution

### 2026-09-27T02:50:43Z · @codex

Receipt: installed boop 0.0.10 (49124370-dirty); read-only store query found 35 retrying, 16 retry_budget_exhausted, and 19 open_failed rows, all with delivery timestamps; that build contains hail_parent_once for both retry transitions.
