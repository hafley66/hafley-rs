---
created: 2026-08-13
updated: 2026-08-13
type: task
status: open
priority: high
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, component-trace-identity, intent-foundation]
lane: boop-identity
lane_seq: 1
collision: [boop-store-schema]
---

# 001 Resolve lane runtime identity

## Description

## Objective

Provide one resolver from lane identity to trace, root session, current harness session, route, process, and completion record.

## Acceptance Criteria

- [ ] Placeholder lane sessions and generated harness sessions are distinguishable.
- [ ] Active session selection uses trace attachments and activity timestamps.
- [ ] Resume, compaction, and session replacement retain one trace.
- [ ] Missing and ambiguous mappings return typed diagnostics.
- [ ] Callers do not join dictionary tables directly.

## Decisions

### 2026-08-14T03:36:51Z · @codex

Consolidated scope: resolve lane to trace and current harness sessions, then attribute cumulative and delta usage across OpenCode generated sessions and Codex session replacement. This task absorbs lane-usage-attribution.
