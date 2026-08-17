---
created: 2026-08-16
updated: 2026-08-16
type: task
status: done
priority: high
epic: soopy-staged-mutations
labels: [domain-soopy, intent-implementation, artifact-runtime]
lane: soopy-mutation-core
lane_seq: 10
collision: [soopy-action-types]
size: M
assignee: terra
commits:
- hash: ee96e75
  summary: Add Soopy source action algebra
closed: 2026-08-16
closed_by: codex
---

# 019 Define source action and edit algebra

## Objective

Define versioned, Git-optional source roots, paths, sources, byte spans, producer ordering, content preconditions, and create/replace/move/delete requests. This task records requests and performs request-local shape validation. Planning, filesystem reads, conflict decisions, output derivation, staging, and writes remain downstream.

## Acceptance Criteria

- [x] Types preserve Git worktree, immutable revision, and plain-directory identities.
- [x] Byte and UTF-8 producer adapters have an explicit boundary.
- [x] Serialization is deterministic and versioned.
- [x] Fixtures preserve duplicate insertions, adjacent edits, overlaps, stale content, moves, creates, and deletes for the planner.

## Tests Run

- [x] `cargo test -p soopy`
- [x] `cargo clippy -p soopy --all-targets -- -D warnings`
- [x] `git diff --check`
