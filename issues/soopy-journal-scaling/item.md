---
created: 2026-08-16
updated: 2026-08-16
type: improvement
assignee: luna
status: done
priority: high
epic: soopy-staged-mutations
labels: [domain-soopy, intent-performance]
closed: 2026-08-16
---

# Bound commit journal write amplification

## Description

## Objective

Remove the measured quadratic journal rewrite path while preserving crash recovery and per-operation progress.

## Acceptance Criteria

- [x] Journal output bytes reuse the sealed stage CAS or another bounded representation.
- [x] Progress persistence does not rewrite the full operation payload after every file.
- [x] The 1000-file 100000-edit receipt records lower journal bytes and elapsed time than the 58-second 12768109-byte baseline.
- [x] Failure-boundary recovery and idempotent replay remain unchanged.

## Tests Run

- [x] cargo test -p soopy
- [x] cargo clippy -p soopy --all-targets -- -D warnings
- [x] just test-source-mutations
- [x] just perf-source-mutations
- [x] git diff --check

## Agent Runs

### 2026-08-16T21:09:13Z · @codex

Merged c89eef6 after removing the corruptible checkpoint sidecar. Recovery derives completed operations from synced target state. Aggregate and strict clippy gates passed. Smoke 100 files and 10000 edits: 2.70 seconds, 59108 journal bytes, zero checkpoint bytes.
