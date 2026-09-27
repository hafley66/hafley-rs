---
created: 2026-08-13
updated: 2026-08-13
type: task
status: obsolete
priority: high
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, artifact-cli, component-usage]
lane: boop-cli
lane_seq: 6
collision: [boop-db-cli]
blocked_by: ['@lane-usage-attribution', '@sync-diagnostics']
closed: 2026-08-13
closed_by: codex
---

# 006 Add lane usage command

## Description

## Objective

Add `boop db lane usage <lane>` for lane-specific cumulative and moving usage.

## Acceptance Criteria

- [ ] Reports cumulative tokens, calls, cache tokens, cost, first activity, and last activity.
- [ ] Reports delta since the preceding successful sync.
- [ ] Reports trailing token-per-minute and cost-per-hour rates.
- [ ] Zero usage distinguishes no model turn from a genuinely zero-valued row.
- [ ] Active OpenCode and Codex fixtures agree with aggregate accounting.

## Resolution

### 2026-08-14T03:36:29Z · @codex

Superseded by lane-status-command, which now owns the complete public boop lane command family.
