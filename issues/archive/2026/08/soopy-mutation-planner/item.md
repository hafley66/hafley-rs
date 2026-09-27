---
created: 2026-08-16
updated: 2026-08-16
type: task
status: done
priority: high
epic: soopy-staged-mutations
labels: [domain-soopy, intent-implementation, artifact-runtime]
lane: soopy-mutation-core
lane_seq: 20
collision: [soopy-action-types, soopy-stage-planner]
size: L
blocked_by: ['@soopy-mutation-algebra']
assignee: terra
commits:
- hash: 275d6ee
  summary: Add Soopy mutation planner
closed: 2026-08-16
closed_by: codex
---

# 020 Normalize and group staged mutations

## Objective

Implement deterministic path normalization, one verified read per source, per-file byte-span grouping, overlap refusal, descending application, resulting content identities, and path-level conflict detection.

## Acceptance Criteria

- [x] Planner changes no target file, Git state, ref, or index.
- [x] Results are independent of producer input order where semantics are unambiguous.
- [x] Same-offset insertions require explicit distinct ordering.
- [x] Git and non-Git fixtures produce equivalent normalized stages.
- [x] Stale content, traversal, foreign roots, immutable mutation targets, path collisions, and occupied move destinations are typed refusals.

## Tests Run

- [x] `cargo test -p soopy`
- [x] `cargo clippy -p soopy --all-targets -- -D warnings`
- [x] `git diff --check`

## Agent Runs

### 2026-08-16T18:34:46Z · @codex

Terra implementation lane in /private/tmp/hafley-soopy-mutation-planner. Includes a 100k-edit scale receipt. Full diff will be reviewed before integration.
