---
created: 2026-08-14
updated: 2026-08-14
type: task
status: in-progress
priority: high
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, component-runtime]
lane: boop-runtime
lane_seq: 13
collision: [boop-runtime-query]
blocked_by: ['@lane-runtime-identity']
assignee: terra
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

- [ ] Returns lane, trace, session, parent, route, cwd, tmux target, pane, PID, liveness, and completion.
- [ ] Includes shell-only registered routes with nullable transcript identity.
- [ ] Returns inbox, outbox, and unacknowledged mailbox counts.
- [ ] Returns worktree coordinates already present in route/pane observations without reading Instant.
- [ ] Uses one tmux session observation and one process snapshot per request.
- [ ] Distinguishes inaccessible tmux from a dead target.
- [ ] Fixtures cover live, dead, shell-only, missing transcript, completion, and stale report cases.

## Tests Run

- [ ] `cargo test -p boop runtime`
- [ ] `cargo test -p boop-mux`
- [ ] `cargo clippy -p boop --lib -- -D warnings`

## Implementation Notes

Build on `runtime.rs`. Boop-only change. Instant remains untouched.

## Agent Runs

### 2026-08-15T00:56:12Z · @codex

Dispatched native Terra in .boop-worktrees/feature/boop-runtime-snapshot. Scope is Boop only; Instant remains unchanged.
