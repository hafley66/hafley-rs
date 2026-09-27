---
created: 2026-08-13
updated: 2026-08-13
type: task
status: obsolete
priority: high
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, artifact-gate, intent-verification]
lane: boop-gates
lane_seq: 9
collision: [boop-observability-fixtures]
blocked_by: ['@lane-status-command', '@lane-usage-command', '@lane-log-tail', '@lane-observability-help']
closed: 2026-08-13
closed_by: codex
---

# 009 Verify lane observability end to end

## Description

## Objective

Create deterministic integration gates for lane identity, tracing, usage, sync diagnostics, and combined status.

## Acceptance Criteria

- [ ] Fixture covers lane placeholder to generated OpenCode session attachment.
- [ ] Fixture covers Codex session replacement on resume or compaction.
- [ ] Fixture covers pre-turn failure with zero usage.
- [ ] Fixture covers active token movement and tool activity.
- [ ] Fixture covers clean completion with report and hail.
- [ ] Fixture covers process death without report or hail.
- [ ] Aggregate usage equals the sum of attributed and explicitly unresolved usage.

## Resolution

### 2026-08-14T03:36:37Z · @codex

Superseded by lane-observability-help, which now owns doctrine and end-to-end gates.
