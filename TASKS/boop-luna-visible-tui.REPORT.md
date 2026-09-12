# Boop visible lane TUI receipt

Status: wip

## Revision

- Base: 2376a81
- Worktree: fix/f41-visible-tui-20260912
- Latest: f41 continuation after 28844a7

## Implementation

The lane supervisor selects a native terminal-backed channel for Claude,
Codex, and OpenCode (`crates/boop-harness/src/0_native_channel.rs`), launched
from each adapter's `open_channel`; Kimi retains ACP. The channel launches the
adapter's `NativeTuiPlan` in the canonical lane pane (frontend inherits
stdin/stdout/stderr), observes the native session, records the native session
identity in the supervisor route, delivers through the adapter door, and keeps
supervisor diagnostics on the existing file logger. Native input targets the
declared tmux socket/target carried through `BOOP_TMUX_SOCKET`/`BOOP_TMUX_TARGET`.

## f41 fixes on top of 28844a7

1. Claude isolated-HOME launch. `claude-ansi` leads PATH and resolves its
   payload under `$HOME/.local/share/claude/versions`, so an isolated lane HOME
   had no binary. `lane_lifecycle_e2e` now pins `CLAUDE_BIN` to the first `claude`
   on PATH whose resolved file is a real executable (not a `#!` wrapper).
2. OpenCode idle wait. `door/opencode.rs::notify_idle` returned a fatal error
   when its event-stream read hit the transport timeout; a finished turn that
   raced past the subscriber never completed. It now treats a missing idle in
   the window as "keep polling" and re-probes session status once before
   answering `stayed active`.
3. OpenCode completion evidence. `0_native_channel.rs::next_event` accepts the
   adapter's completed transcript message (one complete assistant message per
   turn) when the door reports the session stayed active, so completion does not
   depend on catching the `session.idle` event.
4. Resume liveness and busy races. `submit_turn` waits (bounded, 20s) for a
   resumed frontend to register live, and for a transient `busy` status to clear,
   instead of failing the lane.

## Validation

- `cargo test --locked -p boop --test main lane_lifecycle_e2e -- --nocapture --test-threads=1`: 12 passed, 0 failed. Full matrix, four cases (held/revive/retired/stale) x (claude/codex/opencode). Real built Boop, real harness TUIs, isolated scratch tmux/git/store, llmock replay. Log: `/tmp/f41-lane/lane_lifecycle_full3.log`.
- Focused `held_row_defers_result_codex`: pass (initial verification).
- `cargo test --locked -p boop-harness --lib`: 205 passed, 2 ignored.
- `cargo test --locked -p boop-harness --lib door::codex::tests::native_turn_receipt`: 2 passed (status/interrupted and item dedup).
- `cargo test --locked -p boop-harness --lib door::opencode`: 14 passed.
- `cargo fmt --all`, `git diff --check`: clean.

Retained diagnostic evidence from earlier native launches (superseded):
`/var/folders/z2/cwfm40fn65n176q8m227wl0r0000gn/T/boop-lifecycle-held-codex-*`.

## Remaining gates

- TUI placement/util-input regression: assert the lane pane itself renders the
  harness TUI and that typed input reaches the same conversation, for ordinary
  and fork lanes, with pane count/target checks.
- `tui_sigint_e2e`, `commit_push_e2e`, `pr_push_e2e`, `t5_live_harness` matrix.
- Effort/settings compatibility (settings observer events are still dropped).
- Parent + explicit subscriber lifecycle/commit/PR routing/dedup integration.
- Final gate: `just boop-check deterministic`.
- ghcacher PR notification integration (separate commit sequence).

## Routing scope

Existing store contracts route lifecycle results to the parent, commit notices
to the parent and explicit subscribers with (lane, subscriber, head)
deduplication, and PR notices by URL and subscriber. Native-lane integration
evidence for explicit subscribers and commit/PR delivery remains to be run.

No fake harness, fake channel, fake door, provider charge, push, merge, PR
publication, or user-session mutation was used.
