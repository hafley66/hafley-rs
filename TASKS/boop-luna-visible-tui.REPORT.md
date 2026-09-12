# Boop visible lane TUI receipt

Status: wip

## Revision

- Base: 2376a81
- Worktree: fix/f41-visible-tui-20260912
- Latest: 87fda45 + this commit

## Implementation

The lane supervisor selects a native terminal-backed channel for Claude,
Codex, and OpenCode (`crates/boop-harness/src/0_native_channel.rs`), launched
from each adapter's `open_channel`; Kimi retains ACP. The channel launches the
adapter's `NativeTuiPlan` in the canonical lane pane (frontend inherits
stdin/stdout/stderr), observes the native session, records the native session
identity in the supervisor route, delivers through the adapter door, and keeps
supervisor diagnostics on the existing file logger. Native input targets the
declared tmux socket/target carried through `BOOP_TMUX_SOCKET`/`BOOP_TMUX_TARGET`.

## f41 work on top of 28844a7

1. Claude isolated-HOME launch. `claude-ansi` leads PATH and resolves its
   payload under `$HOME/.local/share/claude/versions`, so an isolated lane HOME
   had no binary. `mock_tui::resolve_executable` now prefers the first native
   executable on PATH and falls back to a wrapper only when none exists; an
   explicit `CLAUDE_BIN`/`CODEX_BIN`/`OPENCODE_BIN` still wins.
2. OpenCode idle wait. `door/opencode.rs::notify_idle` returned a fatal error
   when its event-stream read hit the transport timeout; a finished turn that
   raced past the subscriber never completed. It now treats a missing idle in
   the window as "keep polling" and re-probes session status once before
   answering `stayed active`.
3. OpenCode completion evidence. `next_event` accepts the adapter's completed
   transcript message (one complete assistant message per turn) when the door
   reports the session stayed active, so completion does not depend on catching
   the `session.idle` event.
4. Resume liveness and busy races. `submit_turn` waits (bounded, 20s) for a
   resumed frontend to register live and for a transient `busy` status to clear
   instead of failing the lane.
5. Fresh-worktree trust. A lane worktree is new on every spawn, so claude's
   workspace-trust and codex's directory-trust dialogs parked the pane before
   the composer. The channel now marks the lane cwd trusted in the claude
   `$CLAUDE_CONFIG_DIR/.claude.json` / `~/.claude.json` and codex
   `$CODEX_HOME/config.toml` / `~/.codex/config.toml` that the lane reads.
   claude's `--dangerously-skip-permissions` is deliberately not passed: its
   interactive bypass disclaimer is a second gate that parked the ack turn.
6. Commit-push lane reader. The suite's lane env pointed `BOOP_READER_HOME` at
   the coordinator home, so the native channel bound the lane's live session and
   transcript against the wrong registry. The override is gone; the lane's own
   HOME is the reader root.

## Validation

- `cargo test --locked -p boop --test main lane_lifecycle_e2e -- --nocapture --test-threads=1`: 12 passed, 0 failed. Full matrix, four cases (held/revive/retired/stale) x (claude/codex/opencode). Real built Boop, real harness TUIs, isolated scratch tmux/git/store, llmock replay. Log: `~/.cache/boop/lanes/fix-luna-visible-tui-20260912/f41logs/lane_full8.log`. Each case asserts the lane session owns exactly one pane (no second visible log window).
- `cargo test --locked -p boop --test main tui_sigint_e2e`: 3 passed, 0 failed (claude/codex/opencode coordinator Ctrl-C and backend teardown).
- `cargo test --locked -p boop --test main commit_push_e2e`: 4 passed, 0 failed (claude/codex/opencode/kimi lane + coordinator commit watch and delivery).
- `cargo test --locked -p boop-harness --lib`: 205 passed, 2 ignored.
- `cargo test --locked -p boop-harness --lib door::codex::tests::native_turn_receipt`: 2 passed (status/interrupted and item dedup).
- `cargo test --locked -p boop-harness --lib door::opencode`: 14 passed.
- `cargo fmt --all`, `git diff --check`: clean.

## Remaining gates

- `pr_push_e2e`: `both_producers_*` and `hung_gh_*` fail because the native
  channel exposes no `ToolCallFact` set, so the supervisor's title-based
  `gh pr create` discovery never fires. This path is the one the ghcacher brief
  replaces; the fix is either native tool-call extraction from the adapter
  transcript or the ghcacher-driven producer below.
- `t5_live_harness` and `just boop-check deterministic`: not yet run this
  session.
- Effort/settings compatibility: claude/opencode effort reaches the launch;
  `NativeTuiEvent::Settings` observer events are still dropped.
- Parent + explicit subscriber lifecycle/commit/PR routing integration evidence
  beyond commit_push.
- ghcacher PR notification sequence (`boop-ghcacher-pr.BRIEF.md`): deferred; no
  code written this session. It replaces `supervise.rs` `publish_pr` title
  discovery with cached PR state/change events correlated to repo + lane
  branch/head, with per-lane/PR coalescing, generation and dedup, persisted
  cursor, and force-push/rebase equivalence.

No fake harness, fake channel, fake door, provider charge, push, merge, PR
publication, or user-session mutation was used.
