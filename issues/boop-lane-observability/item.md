---
created: 2026-08-13
updated: 2026-09-27
type: epic
owner: chrishafley
status: fixed
priority: high
labels: [domain-boop, intent-observability]
related: ['@boop-pane-liveness']
commits:
- hash: 80d9bd0
  summary: Fix fresh Codex lanes dropping their brief
---

# 000 Boop lane observability

## Description

## Goal

Make one boop command report whether an agent lane is alive, executing model turns, consuming tokens, changing its worktree, producing a report, or exiting.

## Observed Failures

- Lane names are attached as placeholder sessions while OpenCode creates a separate generated session.
- Aggregate usage moves while lane-specific usage reports zero.
- Incremental sync prints `can't find session: <lane>` but completes without attributing the affected rows.
- Supervisor tracing is visible only through tmux pane capture.
- Process, trace, usage, transcript, worktree, report, hail, and exit state require separate commands and raw SQLite queries.

## Acceptance Criteria

- [x] Normal lane monitoring requires no raw SQL or direct tmux commands (`boop beep lane get`).
- [x] Active and completed lanes resolve to their trace and harness sessions.
- [x] Lane-specific token usage and latest-turn deltas are accurate during active turns.
- [x] Structured supervisor and channel logs are retrievable by lane.
- [x] Help documents one canonical monitoring sequence.
- [x] Failure states distinguish pre-model death, active thinking, active tool work, clean completion, and silent death.

## Tests Run

- [x] `cargo nextest run --workspace -j 2 -E "not (test(/e2e|live|tmux|tui_sigint|omp_live/))"` (1337 passed, 199 skipped)
- [ ] traced OpenCode fixture
- [ ] traced Codex fixture

## Reproduction on installed boop 0.0.10 (248dfdd3)

`boop beep lane list`, `boop beep ps`, `boop beep pstree`, and `boop beep lane
get <lane>` remain separate surfaces. The lane detail output has route/session
fields; `ps` has process usage. Help exposes no canonical sequence and the
listed surfaces do not report turn/token deltas, transcript/report progress, or
structured supervisor events together.

## Implementation receipt

`boop beep lane get <lane>` now includes the last 100 structured trace events,
resolved route and trace sessions, lifetime and latest-turn token totals, and
the last 100 supervisor log lines. Its `phase` classifies active thinking,
active tool work, clean completion, pre-model death, silent death, failed
completion, idle, or unknown from persisted trace, transcript and lane state.
`boop --help` documents the canonical command and the corresponding worktree
check.
