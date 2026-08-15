---
created: 2026-08-15
updated: 2026-08-15
type: task
status: open
priority: high
epic: boop-lane-observability
labels: [domain-boop, domain-instant, intent-research, artifact-contract]
lane: boop-contract
related: ['@boop-session-graph', '@instant-boop-migration']
collision: [boop-agent-session-contract]
---

# 015 Audit native harness session graph

## Description

## Objective

Audit the exact native session, parent edge, cwd, tmux, status, and identity semantics emitted by Claude, Codex, OpenCode, and Kimi and consumed by Instant's Harness Trace and external-shell strip. Record corrections directly in `crates/boop/plans/2026-08-15-agent-session-graph.md`.

## Acceptance Criteria

- [ ] All four `Harness::sessions()` implementations are traced to `SessionRef` fields and store relations.
- [ ] Instant's native-descendant subtraction and pane-ownership inputs are enumerated by field and source.
- [ ] Shell-only lanes and harness sessions are kept as distinct identity classes.
- [ ] Missing or ambiguous contract fields are recorded with exact symbols and paths.
- [ ] No Instant or Boop behavior is changed.

## Tests Run

- [ ] `cargo test -p boop harness`
- [ ] Relevant Instant pure join/strip fixture tests identified

## Implementation Notes

Terra research lane. Read-only code audit except corrections to the committed plan and this issue's receipts.
