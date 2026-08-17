---
created: 2026-08-16
updated: 2026-08-16
type: task
status: done
priority: high
epic: soopy-staged-mutations
labels: [domain-soopy, intent-implementation, artifact-runtime]
lane: soopy-stage-store
lane_seq: 30
collision: [soopy-stage-types, soopy-stage-store]
size: M
blocked_by: ['@soopy-mutation-planner']
assignee: luna
closed: 2026-08-16
---

# 021 Persist sealed stages and previews

## Objective

Add pluggable in-memory and durable `StageStore` implementations, content-address staged result bytes, calculate `StageId`, and render deterministic unified previews and operation summaries.

## Acceptance Criteria

- [x] A durable stage survives process restart.
- [x] Presentation formatting does not change `StageId`.
- [x] `show-stage` and `discard-stage` operate without reading target files.
- [x] Stored content is deduplicated and bounded by explicit cleanup policy.

## Tests Run

- [x] `cargo test -p soopy`
- [x] `cargo clippy -p soopy --all-targets -- -D warnings`
- [x] `git diff --check`

## Agent Runs

### 2026-08-16T19:28:45Z · @codex

Luna implementation lane in /private/tmp/hafley-soopy-stage-preview. Includes restart durability and 100k-edit staging scale receipts. Two read-only Luna lanes prepare task 022 commit/recovery and task 024 DL6 binding contracts.

### 2026-08-16T19:54:52Z · @codex

Merged commit 1e6c7eb after correction review. Exact previews, restart and CAS repair tests, full tests, strict clippy, CLI smoke, and the 100k stage receipt passed.

