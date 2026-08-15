---
created: 2026-08-14
updated: 2026-08-14
type: feature
status: open
priority: high
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, artifact-cli]
lane: boop-summary
lane_seq: 14
collision: [boop-agent-summary]
blocked_by: ['@boop-activity-counts', '@boop-runtime-snapshot']
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

- [ ] Returns active-agent count plus per-agent lane, trace, session, liveness, message counts, call counts, tokens, and completion.
- [ ] Joins activity counts and runtime snapshots through stable typed identities.
- [ ] JSON schema is versioned and fixture-tested.
- [ ] Text and JSON output share one Rust result type.
- [ ] CASS issue, reservation, and provider fields are absent and documented as separate.
- [ ] No Instant files are changed.
- [ ] One load fixture covers at least 500 registered lanes without per-row process or tmux acquisition.

## Tests Run

- [ ] `cargo test -p boop agent_summary`
- [ ] `cargo clippy -p boop --all-targets -- -D warnings`
- [ ] 500-lane bounded-acquisition receipt

## Implementation Notes

This is the Boop-side replacement boundary. Instant migration remains a later issue.
