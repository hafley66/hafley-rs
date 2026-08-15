---
created: 2026-08-14
updated: 2026-08-14
type: feature
status: done
priority: high
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, artifact-cli]
lane: boop-summary
lane_seq: 14
collision: [boop-agent-summary]
blocked_by: ['@boop-activity-counts', '@boop-runtime-snapshot']
assignee: terra
commits:
- hash: a1fdc60
  summary: expose-versioned-agent-summary
closed: 2026-08-14
---

# 014 Expose CASS-compatible Boop agent summary

## Description

## Objective

Expose a stable Boop JSON summary that covers the agent/runtime/message/call portion currently obtained from CASS, while keeping CASS issue, reservation, and provider data outside this contract.

## Type Signature

```rust
pub fn agent_summary(query: AgentSummaryQuery) -> Result<AgentSummary>;
```

The CLI serializes this row without exposing SQLite storage details.

## Acceptance Criteria

- [x] Returns active-agent count plus per-agent lane, trace, session, liveness, message counts, call counts, tokens, and completion.
- [x] Joins activity counts and runtime snapshots through stable typed identities.
- [x] JSON schema is versioned and fixture-tested.
- [x] Text and JSON output share one Rust result type.
- [x] CASS issue, reservation, and provider fields are absent and documented as separate.
- [x] No Instant files are changed.
- [x] A multi-lane fixture proves one shared process and tmux observation per summary request.

## Tests Run

- [x] `cargo test -p boop summary`
- [x] `cargo clippy -p boop --all-targets -- -D warnings -A clippy::large-enum-variant`
- [x] Multi-lane bounded-acquisition receipt

## Implementation Notes

This is the Boop-side replacement boundary. Instant migration remains a later issue.

Strict all-target clippy passes with the existing `BeepCmd` large-enum-variant lint allowed. Strict library clippy passes without allowances. The repository-wide test run has 169 passing tests and three existing lane model-routing failures unrelated to this change.

## Agent Runs

### 2026-08-15T01:23:04Z · @codex

Dispatched native Terra in .boop-worktrees/feature/boop-cass-summary. Integrate the merged activity and runtime projections into the versioned Boop library and CLI summary. Instant remains unchanged.
