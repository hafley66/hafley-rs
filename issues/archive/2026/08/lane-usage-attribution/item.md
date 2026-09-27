---
created: 2026-08-13
updated: 2026-08-13
type: task
status: obsolete
priority: high
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, component-usage, component-store]
lane: boop-telemetry
lane_seq: 3
collision: [boop-store-schema]
blocked_by: ['@lane-runtime-identity']
closed: 2026-08-13
closed_by: codex
---

# 003 Attribute usage to lanes

## Description

## Objective

Attribute harness usage rows to a lane trace while the active session identifier differs from the lane placeholder.

## Acceptance Criteria

- [ ] OpenCode generated sessions contribute to their owning lane.
- [ ] Codex session replacement and compaction remain attached.
- [ ] Totals expose input, output, cache creation, cache reads, calls, and recorded cost.
- [ ] Usage supports cumulative values and deltas since the preceding sync.
- [ ] Sidechain usage remains identifiable.

## Resolution

### 2026-08-14T03:36:20Z · @codex

Superseded by lane-runtime-identity, which now owns session resolution and usage attribution.
