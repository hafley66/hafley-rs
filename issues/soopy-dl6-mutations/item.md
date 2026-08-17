---
created: 2026-08-16
updated: 2026-08-16
type: task
status: done
priority: high
epic: soopy-staged-mutations
labels: [domain-soopy, intent-implementation, artifact-runtime]
lane: sprefa-dl6-host
lane_seq: 50
collision: [soopy-stage-types, sprefa-host-schema]
size: L
blocked_by: ['@soopy-mutation-commit', '@soopy-edit-producers']
assignee: terra
closed: 2026-08-16
---

# 024 Bind DL6 proposals and approvals

## Objective

Expose source actions, proposed files, conflicts, staged previews, exact `StageId` approval, commit demand, and commit receipt as DL6 host relations. Add combined correctness and performance gates to the Soopy justfile.

## Acceptance Criteria

- [x] DL6 can join findings with source, Git, dependency, ownership, and type facts before proposing edits.
- [x] Dataflow reaches quiescence while waiting for approval.
- [x] Approval of one `StageId` cannot release another stage.
- [x] Commit receipts return as ordinary facts on a later tick.
- [x] `just test-source-mutations` covers end-to-end producer through receipt.
- [x] `just perf-source-mutations` records edit, file, repository, elapsed, allocation, and RSS measurements.

## Tests Run

- [x] `cargo test -p soopy`
- [x] `cargo clippy -p soopy --all-targets -- -D warnings`
- [x] `just test-source-mutations`
- [x] `just perf-source-mutations`

## Agent Runs

### 2026-08-16T20:55:15Z · @codex

Merged Sprefa commits f3e48d4f8 and 05ed014c3, plus hafley-rs gate commit ea59cb3. Compiler fixture, Rust host 5, TypeScript capability 1, aggregate pipeline, full Soopy tests, strict clippy, and 100000-edit receipt passed. Measured journal amplification is tracked separately as soopy-journal-scaling.
