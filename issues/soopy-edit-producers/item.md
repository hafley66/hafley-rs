---
created: 2026-08-16
updated: 2026-08-16
type: task
status: done
priority: high
epic: soopy-staged-mutations
labels: [domain-soopy, intent-implementation, artifact-runtime]
lane: soopy-edit-adapters
lane_seq: 30
collision: [soopy-action-types]
size: M
blocked_by: ['@soopy-mutation-algebra']
assignee: luna
commits:
- hash: c114082
  summary: Adapt structural edit producers
closed: 2026-08-16
closed_by: codex
---

# 023 Adapt structural edit producers

## Objective

Translate ast-grep-core edits, Biome `BatchMutation` output, and rust-analyzer text edits into one `ProducedEdit` envelope while preserving producer and rule provenance.

## Acceptance Criteria

- [x] Adapters contain no filesystem mutation.
- [x] Existing custom rules can emit normalized byte edits without changing their match engines.
- [x] Equivalent edits deduplicate with all provenance retained.
- [x] Conflicting producer outputs reach the planner as typed conflicts.

## Tests Run

- [x] `cargo test -p soopy`
- [x] `cargo clippy -p soopy --all-targets -- -D warnings`
- [x] `git diff --check`

## Agent Runs

### 2026-08-16T18:34:48Z · @codex

Luna implementation lane in /private/tmp/hafley-soopy-edit-producers. Includes a 100k-edit adapter scale receipt and explicit executable-versus-contract adapter boundaries.
