---
created: 2026-08-13
updated: 2026-08-13
type: task
status: obsolete
priority: high
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, component-sync, intent-diagnostics]
lane: boop-telemetry
lane_seq: 4
collision: [boop-sync]
blocked_by: ['@lane-runtime-identity']
closed: 2026-08-13
closed_by: codex
---

# 004 Diagnose incremental sync gaps

## Description

## Objective

Replace context-free `can't find session` output with structured synchronization diagnostics.

## Acceptance Criteria

- [ ] Every unresolved session reports source path, harness, external id, lane or trace candidate, and affected event count.
- [ ] Sync success and partial attribution are distinct result states.
- [ ] dropped, sparse, unresolved, inserted, and updated counts are machine-readable.
- [ ] Re-sync after attachment resolves rows idempotently.
- [ ] Diagnostics are covered by fixtures.

## Resolution

### 2026-08-14T03:36:23Z · @codex

Superseded by lane-tracing-events, which now owns structured sync diagnostics.
