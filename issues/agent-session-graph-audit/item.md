---
created: 2026-08-15
updated: 2026-08-15
type: task
status: done
priority: high
epic: boop-lane-observability
labels: [domain-boop, domain-instant, intent-research, artifact-contract]
lane: boop-contract
related: ['@boop-session-graph', '@instant-boop-migration']
collision: [boop-agent-session-contract]
closed: 2026-08-15
---

# 015 Audit native harness session graph

## Description

## Objective

Audit the exact native session, parent edge, cwd, tmux, status, and identity semantics emitted by Claude, Codex, OpenCode, and Kimi and consumed by Instant's Harness Trace and external-shell strip. Record corrections directly in `crates/boop/plans/2026-08-15-agent-session-graph.md`.

## Acceptance Criteria

- [x] All four `Harness::sessions()` implementations are traced to `SessionRef` fields and store relations.
- [x] Instant's native-descendant subtraction and pane-ownership inputs are enumerated by field and source.
- [x] Shell-only lanes and harness sessions are kept as distinct identity classes.
- [x] Missing or ambiguous contract fields are recorded with exact symbols and paths.
- [x] No Instant or Boop behavior is changed.

## Tests Run

- [x] `cargo test -p boop harness`
- [x] Relevant Instant pure join/strip fixture tests identified

## Implementation Notes

Terra research lane. Read-only code audit except corrections to the committed plan and this issue's receipts.

## Agent Runs

### 2026-08-15T16:19:06Z · @codex

Audit recorded in crates/boop/plans/2026-08-15-agent-session-graph.md. Focused Boop harness test passed: 45 library tests and 2 binary tests; Instant fixture commands are recorded but not run because the Instant worktree is read-only and dirty.
