---
created: 2026-08-15
updated: 2026-08-15
type: task
status: open
priority: high
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, artifact-api, artifact-cli]
lane: boop-query
related: ['@agent-session-graph-audit', '@instant-boop-migration']
collision: [boop-agent-session-graph]
---

# 016 Expose typed agent session graph

## Description

## Objective

Implement the typed, set-wise agent session graph described by `crates/boop/plans/2026-08-15-agent-session-graph.md`, including native parent edges and shell-only runtime rows.

## Acceptance Criteria

- [x] A typed library query returns schema version, session nodes, parent edges, and shell-only lane nodes.
- [x] The query uses set-wise store reads and one bounded runtime observation.
- [x] Claude, Codex, OpenCode, and Kimi parent fixtures project through the same relation.
- [x] `boop agent sessions [--cwd <path>] [--history] --format json` emits one JSON document.
- [x] Public help contains no `swarm` vocabulary.
- [x] Existing `boop agent summary` behavior remains covered.

## Tests Run

- [x] `cargo test -p boop`
- [x] `cargo clippy -p boop --all-targets -- -D warnings`
- [x] `issuectl doctor`

## Implementation Notes

Luna implementation lane. Work only in a dedicated Hafley worktree. Commit the implementation without pushing or merging.

Correction notes: current native scope retains discovered sessions unless their
durable state is explicitly `dead`; shell scope is route-qualified and requires
shell route evidence. Public session and edge identities are `{harness, id}`.
The existing bare-string `dict_session` key can have already-collided rows, so
recovering those historical collisions remains a storage migration deferral.
Public edges require both endpoints in the filtered session set; dangling
provider parent edges remain durable but are omitted from JSON.
