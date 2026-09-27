---
created: 2026-08-13
updated: 2026-08-13
type: task
status: obsolete
priority: high
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, artifact-cli, component-tracing]
lane: boop-cli
lane_seq: 7
collision: [boop-db-cli]
blocked_by: ['@lane-runtime-identity', '@lane-tracing-events']
closed: 2026-08-13
closed_by: codex
---

# 007 Add lane log tail command

## Description

## Objective

Add `boop db lane tail <lane>` for recent structured supervisor, channel, and harness activity.

## Acceptance Criteria

- [ ] Default output is bounded and ordered.
- [ ] Level, component, event, session, turn, and timestamp filters are available.
- [ ] Text and JSON formats are supported.
- [ ] Follow mode terminates after lane completion unless explicitly retained.
- [ ] Users do not need tmux capture-pane for diagnosis.

## Resolution

### 2026-08-14T03:36:34Z · @codex

Superseded by lane-status-command, which now owns the complete public boop lane command family.
