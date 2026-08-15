---
created: 2026-08-14
updated: 2026-08-14
type: task
status: done
priority: high
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, component-runtime]
lane: boop-runtime
lane_seq: 13
collision: [boop-runtime-query]
blocked_by: ['@lane-runtime-identity']
assignee: terra
commits:
- hash: 7863f04
  summary: project-bounded-agent-runtime-state
closed: 2026-08-14
---

# 013 Project bounded agent runtime snapshot

## Description

## Objective

Project route, mailbox, tmux, process, shell-only, completion, and worktree facts into one bounded typed runtime observation.

## Type Signature

```rust
pub fn runtime_snapshot(input: RuntimeSnapshotInput) -> Result<Vec<AgentRuntimeRow>>;
```

The body observes tmux and processes once, folds mailbox rows once, and joins those observations to resolved lane identities.

## Acceptance Criteria

- [x] Returns lane, trace, session, parent, route, cwd, tmux target, pane, PID, liveness, and completion.
- [x] Includes shell-only registered routes with nullable transcript identity.
- [x] Returns inbox, outbox, and unacknowledged mailbox counts.
- [x] Returns worktree coordinates already present in route/pane observations without reading Instant.
- [x] Uses one tmux session observation and one process snapshot per request.
- [x] Distinguishes inaccessible tmux from a dead target.
- [x] Fixtures cover live, dead, shell-only, missing transcript, completion, and stale report cases.

## Tests Run

- [x] `cargo test -p boop runtime`
- [x] `cargo test -p boop-mux`
- [x] `cargo clippy -p boop --lib -- -D warnings`

## Implementation Notes

Build on `runtime.rs`. Boop-only change. Instant remains untouched.

## Agent Runs

### 2026-08-15T00:56:12Z · @codex

Dispatched native Terra in .boop-worktrees/feature/boop-runtime-snapshot. Scope is Boop only; Instant remains unchanged.
