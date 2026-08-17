---
created: 2026-08-17
updated: 2026-08-17
type: bug
status: fixed
priority: high
epic: boop-lane-observability
related: ['@boop-runtime-snapshot', '@instant-boop-migration']
labels: [domain-boop, component-runtime, component-tmux]
lane: boop-runtime
lane_seq: 14
collision: [boop-runtime-query]
commits:
- hash: feceaa7
  summary: Fix pane routes reported dead
- hash: 2e861a9
  summary: keep pane and native routes live
closed: 2026-08-17
---

# Keep registered tmux panes live

## Description

## Objective

Make registered tmux pane routes such as `%590` remain live in every Boop projection and ensure the installed CLI cannot silently regress to a build without the pane-target fix.

## Acceptance Criteria

- [x] `target_alive` accepts tmux `%pane` targets and session targets.
- [x] `boop beep lane list`, runtime snapshots, and route queries agree on pane liveness.
- [x] Tests use a real temporary tmux server and pane ID.
- [x] The pane fix is integrated into the main hafley-rs branch without overwriting unrelated dirty-tree changes.
- [x] The canonical install/build path produces a binary containing the pane fix.
- [x] Reinstalling from the main checkout cannot replace the fixed binary with the prior session-only implementation.
- [x] An Instant poll fixture observes a registered live pane as live.

## Tests Run

- [x] `cargo test -p boop-mux target_alive_tracks_a_live_pane_and_drops_a_dead_session`
- [x] `cargo test -p boop tests::dead_reason_is_none_for_a_live_session`
- [x] `git diff --check`
- [x] Full `cargo test -p boop-mux`
- [x] Full `cargo test -p boop`

## Implementation Notes

Worktree commit `feceaa7` contains the verified pane-target implementation. The same hunks were applied to a dirty main checkout but were left uncommitted because `crates/boop/src/main.rs` contained unrelated user changes. The installed binary was later overwritten by an older build and had to be reinstalled from the worktree.

## Decisions

### 2026-08-17T12:41:13Z · @codex

Scope reconciliation after native-agent test: `boop beep agent register <name> --parent <coordinator>` creates a pane-less native route, but lane projection reports `dead/no-trail`; `boop wait --me` then accepted a premature `done rc=1` row while the Codex collaboration agent remained running. This issue now also owns native-route liveness and completion semantics: native registration remains live until explicit `agent done`, wait returns only valid unread completion, and coordinator delivery remains addressable.

## Reopen Notes — 2026-08-17

_Add rationale for reopening here._

## Agent Runs

### 2026-08-17T13:18:45Z · @codex

Reopened after installed-main smoke test: tmux display confirms %590 exists with pane PID 37604, while boop beep ps codex-590 invokes has-session against %590 and reports PID 0. pane_pid target classification still treats percent-pane targets as session names.

### 2026-08-17T13:26:19Z · @codex

Integrated cace514, passed 12 boop-mux tests, reinstalled Boop from current main, and smoke-tested the live coordinator route. boop beep ps codex-590 now reports pane PID 37604, RSS 474912 KiB, and 13 descendants.

