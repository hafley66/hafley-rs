---
created: 2026-08-16
updated: 2026-09-26
type: task
assignee: luna
status: open
priority: high
epic: soopy-staged-mutations
labels: [domain-soopy, intent-performance]
---

# Scale concurrent repository refresh memory

## Description

## Objective

Measure and bound Soopy memory, process count, and latency while many independent Git repositories refresh in the background.

## Acceptance Criteria

- [x] A deterministic local-remote fixture covers many repositories without network access.
- [x] Refresh, fetch, ref, worktree, and watcher paths run concurrently with an explicit inflight cap.
- [x] Receipt records requested and effective repository count, refresh rounds, Git child count, elapsed phases, RSS samples, peak RSS, and retained cache bytes.
- [x] Warm rounds demonstrate bounded RSS rather than per-round corpus retention.
- [x] Batching audit identifies every per-repository process boundary and its concurrency cap.
- [ ] 100-repository scale measurement: `cargo run --release -p soopy --example 6_multi_repo_refresh -- --repositories 100 --rounds 3 --concurrency 4`.

## Tests Run

- [x] `cargo nextest run -p soopy -j 2 --test main -E 'test(t16_multi_repo_refresh::)'` (1 passed).
- [ ] 100-repository measurement command above.
- [x] `git diff --check`.

## Repro receipt

2026-09-26: targeted local-remote fixture test passed (8 repositories, 2 rounds, concurrency cap 2); receipt includes phase times, child count, RSS samples, and retained-cache bounds. The 100-repository run remains unchecked; exact command is above.
