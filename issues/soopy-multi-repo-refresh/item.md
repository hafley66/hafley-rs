---
created: 2026-08-16
updated: 2026-08-16
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

- [ ] A deterministic local-remote fixture covers many repositories without network access.
- [ ] Refresh, fetch, ref, worktree, and watcher paths run concurrently with an explicit inflight cap.
- [ ] Receipt records requested and effective repository count, refresh rounds, Git child count, elapsed phases, RSS samples, peak RSS, and retained cache bytes.
- [ ] Warm rounds demonstrate bounded RSS rather than per-round corpus retention.
- [ ] Batching audit identifies every per-repository process boundary and its concurrency cap.

## Tests Run

- [ ] cargo test -p soopy
- [ ] cargo clippy -p soopy --all-targets -- -D warnings
- [ ] just test-multi-repo-refresh
- [ ] just perf-multi-repo-refresh
- [ ] git diff --check
