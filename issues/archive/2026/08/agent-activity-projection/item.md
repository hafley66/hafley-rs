---
created: 2026-08-14
updated: 2026-08-14
type: feature
status: obsolete
priority: high
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, artifact-api, artifact-cli]
lane: boop-query
lane_seq: 10
collision: [boop-store-schema]
blocked_by: ['@instant-agent-contract', '@lane-runtime-identity', '@lane-tracing-events']
closed: 2026-08-14
---

# 010 Expose typed agent activity projection

## Description

## Objective

Expose one typed Boop projection for the agent activity fields Instant currently derives from CASS, harness transcript stores, mailbox rows, tmux, and multiple Boop commands.

## Type Signature

```rust
pub fn agent_activity(query: AgentActivityQuery) -> Result<Vec<AgentActivity>, BoopError>;
```

The body synchronizes transcript facts once, aggregates by requested session, trace, or lane, joins runtime state once, and returns stable JSON through the CLI.

## Acceptance Criteria

- [ ] Returns stable IDs for lane, trace, current session, parent session, harness, route, tmux target, and cwd.
- [ ] Returns user, assistant, tool-call, and total turn counts with documented counting rules.
- [ ] Returns token totals, first activity, last activity, completion hail, exit code, and current runtime classification.
- [ ] Covers shell-only registered lanes that have no harness transcript.
- [ ] Uses a bounded set-wise tmux/process observation rather than one command per row.
- [ ] SQLite schema additions occur only for missing raw facts; aggregates remain query code or views.
- [ ] Text and JSON CLI forms derive from the same Rust rows.
- [ ] Fixtures cover compaction, resume, session replacement, parent edges, dead tmux, and incomplete launch.

## Tests Run

- [ ] `cargo test -p boop`
- [ ] `cargo test -p boop-mux`
- [ ] `cargo clippy -p boop --all-targets -- -D warnings`

## Implementation Notes

Reuse `agent_turn`, `agent_usage`, `agent_trace_span`, `agent_lane`, `agent_edge`, `agent_live`, and mailbox/runtime acquisition. Do not expose SQLite table IDs to Instant.

## Decisions

### 2026-08-15T02:01:56Z · @codex

Scope absorbed by completed boop-activity-counts, boop-runtime-snapshot, and boop-cass-summary. No separate implementation remains.
