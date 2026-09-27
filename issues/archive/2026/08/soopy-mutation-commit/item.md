---
created: 2026-08-16
updated: 2026-08-16
type: task
status: done
priority: high
epic: soopy-staged-mutations
labels: [domain-soopy, intent-implementation, artifact-runtime]
lane: soopy-stage-store
lane_seq: 40
collision: [soopy-stage-types, soopy-stage-store, soopy-fs-write]
size: L
blocked_by: ['@soopy-stage-preview']
assignee: luna
closed: 2026-08-16
---

# 022 Commit and recover staged mutations

## Objective

Revalidate every staged precondition, acquire a root-scoped writer lock, apply per-file atomic replacements and path operations, record a write-ahead recovery journal, observe resulting identities, and return a typed receipt.

## Acceptance Criteria

- [x] A stale input refuses the transaction before target mutation.
- [x] Git index, refs, and commits remain unchanged.
- [x] Plain directories and Git worktrees share the commit path.
- [x] Injected failure at every operation boundary produces deterministic recovery.
- [x] Replaying a completed `StageId` is explicitly idempotent or explicitly refused.

## Tests Run

- [x] `cargo test -p soopy`
- [x] `cargo clippy -p soopy --all-targets -- -D warnings`
- [x] `just test-source-mutations`
- [x] `git diff --check`

## Agent Runs

### 2026-08-16T19:55:33Z · @codex

Luna implementation lane started in /private/tmp/hafley-soopy-mutation-commit from main 1e6c7eb. Scope is commit, recovery, confinement, receipts, failpoints, watcher correlation, and scale.

### 2026-08-16T20:22:23Z · @codex

Merged 371f84b after two correction reviews. Focused commit tests 8. Aggregate gates planner 10 stage 6 commit 8. Strict clippy and diff check passed. Scale 100 files in 2.04 seconds with 51448 journal bytes.

