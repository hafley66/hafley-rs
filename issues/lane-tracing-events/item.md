---
created: 2026-08-13
updated: 2026-08-13
type: task
status: open
priority: high
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, component-supervisor, component-tracing]
lane: boop-telemetry
lane_seq: 2
collision: [boop-tracing]
blocked_by: ['@lane-runtime-identity']
---

# 002 Persist lane tracing events

## Description

## Objective

Make existing tracing events queryable by lane without tmux pane capture.

## Acceptance Criteria

- [ ] Supervisor, channel-open, turn-start, turn-finish, error, and exit events retain lane and trace identity.
- [ ] RUST_LOG filtering remains supported.
- [ ] Trace persistence has a bounded retention policy.
- [ ] Secrets and complete prompt bodies are excluded from log fields.
- [ ] A failed pre-turn launch leaves an error event.

## Decisions

### 2026-08-14T03:36:55Z · @codex

Consolidated scope: persist supervisor/channel/turn/error/exit tracing by lane and expose structured incremental-sync diagnostics for unresolved sessions. This task absorbs sync-diagnostics.

### 2026-08-14T03:53:19Z · @codex

Observed 2026-08-13 on feature-soopy-source-identities-retry: OpenCode displayed a completed final answer and the worktree had clean committed results, but the supervisor emitted no turn-finish or lane-exit event and stayed alive in turn-start for more than 30 seconds. Two queued steering hails, including an explicit instruction to send a result hail, were never delivered. Completion detection and queued-message delivery must be tested against this final-answer state.

