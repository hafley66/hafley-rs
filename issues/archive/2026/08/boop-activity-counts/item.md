---
created: 2026-08-14
updated: 2026-08-14
type: task
status: done
priority: high
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, component-query]
lane: boop-activity
lane_seq: 12
collision: [boop-store-query]
blocked_by: ['@lane-runtime-identity']
assignee: terra
commits:
- hash: 01feafc
  summary: project-normalized-agent-activity
closed: 2026-08-14
---

# 012 Project normalized agent activity counts

## Description

## Objective

Project normalized Boop transcript and usage facts into typed per-session, per-trace, and per-lane activity counts.

## Type Signature

```rust
pub fn activity_counts(store: &Store, scope: ActivityScope) -> Result<Vec<ActivityCount>>;
```

The body performs set-wise SQL over turns, usage, trace attachments, and resolved lane identity. It does not read Instant or CASS.

## Acceptance Criteria

- [x] Counts user, assistant, tool-call, and total normalized turns.
- [x] Aggregates the same facts by session, trace, and lane without double counting replacement sessions.
- [x] Returns token buckets, request-call count, first activity, and last activity.
- [x] Documents current tool-result omission as an explicit availability field.
- [x] Uses typed public rows with no dictionary or SQLite IDs.
- [x] Query-plan tests reject per-session N+1 reads.
- [x] Claude, Codex, OpenCode, Kimi, resume, and replacement fixtures are deterministic.

## Tests Run

- [x] `cargo test -p boop activity`
- [x] `cargo clippy -p boop --lib -- -D warnings`

## Implementation Notes

Boop-only change. Instant remains untouched.

## Agent Runs

### 2026-08-15T00:56:12Z · @codex

Dispatched native Terra in .boop-worktrees/feature/boop-activity-counts. Scope is Boop only; Instant remains unchanged.
