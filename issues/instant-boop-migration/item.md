---
created: 2026-08-14
updated: 2026-08-15
type: task
status: open
priority: high
epic: boop-lane-observability
labels: [domain-instant, domain-boop, intent-migration, artifact-integration]
lane: instant-consumer
lane_seq: 11
collision: [instant-agent-panels]
blocked_by: ['@agent-activity-projection', '@lane-status-command', '@agent-session-graph-audit', '@boop-session-graph']
---

# 011 Migrate Instant agent panels to Boop

## Description

## Objective

Move Instant's Agents and Harness Trace data acquisition onto the typed Boop activity projection, then remove duplicated CASS agent-message parsing and direct harness-ledger joins.

## Type Signature

```ts
type LoadAgentActivity = (query: AgentActivityQuery) => Promise<AgentActivity[]>;
```

The body invokes one structured Boop command through the existing runner boundary, converts rows into panel state, and leaves terminal layout and rendering inside Instant.

## Acceptance Criteria

- [ ] Agents and Harness Trace panels consume stable Boop JSON rather than parsing text, TSV, and NDJSON from several commands.
- [ ] Per-session, per-trace, and per-lane message counts match pinned current fixtures.
- [ ] Tmux, shell-only, parent, route, liveness, and completion display match pinned current fixtures.
- [ ] Instant no longer reads Claude, Codex, OpenCode, or Kimi transcript stores for covered fields.
- [ ] Instant no longer invokes `cass swarm status` for agent messages or calls.
- [ ] CASS provider, issue, and reservation UI is retained or removed through an explicit field-by-field decision in the contract.
- [ ] Boop SQLite paths and table names do not enter TypeScript types.
- [ ] One integration fixture exercises Boop JSON through the Instant row model.

## Tests Run

- [ ] Instant Vitest agent and harness-trace suites
- [ ] Instant Rust harness-store tests after removal of duplicated readers
- [ ] Boop and Instant integration fixture

## Implementation Notes

Instant owns panel state, terminal layout, and rendering. Boop owns harness acquisition, transcript normalization, trace/lane identity, runtime observation, and activity derivation.

## Comments

### 2026-08-15T16:14:37Z · @codex

Session-graph contract and task split live in crates/boop/plans/2026-08-15-agent-session-graph.md. Replace cass_swarm_status vocabulary and acquisition with boop agent sessions JSON; preserve Instant-owned native-descendant subtraction and pane ownership.
