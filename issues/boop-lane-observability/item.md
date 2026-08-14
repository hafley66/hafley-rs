---
created: 2026-08-13
updated: 2026-08-13
type: epic
owner: chrishafley
status: open
priority: high
labels: [domain-boop, intent-observability]
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

- [ ] Normal lane monitoring requires no raw SQL or direct tmux commands.
- [ ] Active and completed lanes resolve to their trace and harness sessions.
- [ ] Lane-specific token usage and deltas are accurate during active turns.
- [ ] Structured supervisor and channel logs are retrievable by lane.
- [ ] Help documents one canonical monitoring sequence.
- [ ] Failure states distinguish pre-model death, active thinking, active tool work, clean completion, and silent death.

## Tests Run

- [ ] cargo test -p boop
- [ ] cargo test -p boop-mux
- [ ] traced OpenCode fixture
- [ ] traced Codex fixture
