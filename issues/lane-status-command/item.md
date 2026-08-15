---
created: 2026-08-13
updated: 2026-08-14
type: task
status: open
priority: high
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, artifact-cli, component-status]
lane: boop-cli
lane_seq: 5
collision: [boop-db-cli]
blocked_by: ['@lane-runtime-identity', '@lane-tracing-events']
---

# 005 Add lane status command

## Description

## Objective

Add `boop db lane status <lane>` as the canonical combined liveness view.

## Output

process_alive, trace, current_session, turns, tool_calls, input_tokens, output_tokens, last_activity_ts, worktree_changed, report_exists, completion_hail, and exit_code.

## Acceptance Criteria

- [ ] Command performs an incremental sync before reading state.
- [ ] Text and JSON formats carry the same fields.
- [ ] State classification covers queued, starting, active-thinking, active-tools, completed, failed, and silent-death.
- [ ] No raw SQL or direct tmux access is required.

## Decisions

### 2026-08-14T03:36:59Z · @codex

Public CLI is `boop lane status|usage|logs|list|wait`. `db` remains an implementation detail. This task also exposes queryable message events through `boop beep list|show|tail`; lane wait stays under lane. This task absorbs lane-usage-command and lane-log-tail.

### 2026-08-14T03:49:02Z · @codex

Observed 2026-08-13: `boop beep lane wait --timeout N <lane>` returned control to the caller while leaving background wait processes alive. Three orphan waits accumulated for one lane and required targeted termination. Public `boop lane wait` must block or time out with one owned process and leave no orphan.

## Comments

### 2026-08-15T00:19:50Z · @codex

Observed 2026-08-14: two read-only audit lanes exited rc=1 before attaching a supervisor conversation. Their worktrees retained unrelated stale REPORT.md files from prior use, so report existence alone was false evidence. Lane status must report spawn/run failure and report provenance or freshness.
