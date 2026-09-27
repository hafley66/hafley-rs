---
created: 2026-08-13
updated: 2026-08-17
type: task
status: done
priority: high
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, component-supervisor, component-tracing]
lane: boop-telemetry
lane_seq: 2
collision: [boop-tracing]
blocked_by: ['@lane-runtime-identity']
assignee: luna
closed: 2026-08-17
---

# 002 Persist lane tracing events

## Description

## Objective

Make existing tracing events queryable by lane without tmux pane capture.

## Acceptance Criteria

- [x] Supervisor, channel-open, turn-start, turn-finish, error, and exit events retain lane and trace identity.
- [x] RUST_LOG filtering remains supported.
- [x] Trace persistence has a bounded retention policy.
- [x] Secrets and complete prompt bodies are excluded from log fields.
- [x] A failed pre-turn launch leaves an error event.

## Decisions

### 2026-08-14T03:36:55Z · @codex

Consolidated scope: persist supervisor/channel/turn/error/exit tracing by lane and expose structured incremental-sync diagnostics for unresolved sessions. This task absorbs sync-diagnostics.

### 2026-08-14T03:53:19Z · @codex

Observed 2026-08-13 on feature-soopy-source-identities-retry: OpenCode displayed a completed final answer and the worktree had clean committed results, but the supervisor emitted no turn-finish or lane-exit event and stayed alive in turn-start for more than 30 seconds. Two queued steering hails, including an explicit instruction to send a result hail, were never delivered. Completion detection and queued-message delivery must be tested against this final-answer state.

### 2026-08-17T02:25:38Z · @codex

The temporal consumer requires stable event identity, lane and trace identity, causal endpoint identity where applicable, event kind, timestamp or interval bounds, delivery state, and completion or death classification. Missing timestamps remain absent. This extends the existing structured tracing acceptance criteria and does not create a separate telemetry producer.

## Agent Runs

### 2026-08-17T12:55:59Z · @codex

Dispatched gpt-5.6-luna at high reasoning in .boop-worktrees/feature/lane-tracing-events-luna from main 2e861a9. Any major public trait, database schema, or durable data-model change requires an explicit signature/table/migration checkpoint before implementation.

### 2026-08-17T13:15:19Z · @codex

Integrated as 6ac9ef6 after schema/signature and exact-once exit review. Main verification passed: cargo test -p boop (248 library tests, 35 binary tests, integration suites), cargo test -p boop-mux (11 tests), and cargo check --workspace.

### 2026-08-17T13:55:34Z · @codex

Follow-up integrated as 4ac4938. The public schema-version-1 session graph now includes a bounded trace_events array with cwd/history filtering and legacy default compatibility. Installed CLI receipt confirms the field is present; deterministic filtered JSON test passed.


