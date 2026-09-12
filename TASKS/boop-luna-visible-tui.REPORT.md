# Boop visible lane TUI receipt

Status: wip

## Revision

- Base: 2376a81
- Worktree: fix/luna-visible-tui-20260912
- Checkpoint: native lane channel and isolated tmux input routing

## Implementation

The lane supervisor now selects a native terminal-backed channel for Claude,
Codex, and OpenCode. The channel launches the adapter's existing
NativeTuiPlan, inherits the canonical pane terminal, observes the native
session, uses the adapter door for delivery and idle events, and records the
native session identity in the existing supervisor route. Kimi retains ACP.
Supervisor diagnostics remain in supervise.log. Native input uses the
declared isolated tmux socket carried through BOOP_TMUX_SOCKET.

The first native Codex attempt recorded "startup acknowledgment timed out
after 30s"; the session was created but the initial input addressed the default
tmux server. After socket propagation, Codex created a native session, then
reported "thread ... is not materialized yet; thread/turns/list is unavailable
before first user message". The channel now falls back to the native pane for
that first user message and uses the door after materialization.

## Startup reliability

The dispatched startup attempt ran just boop-start in a fresh external target,
exceeded Boop's 120-second SPAWN_CHILD_TIMEOUT, and was killed while building.
The parent retried with --no-start. The bounded warmup test passed with 5 cases,
including shared target reuse. Cold-build launch behavior remains under
integration verification.

## Validation

- cargo check --locked -p boop-harness -p boop: pass.
- cargo fmt --all and git diff --check: pass.
- cargo test --locked -p boop --test main boop_start_warm -- --nocapture --test-threads=1: 5 passed.
- Native Codex lifecycle regression: failed before the materialization fallback; rerun required after the latest change.
- Earlier baseline TUI, commit, PR, and lifecycle suites retained failures and do not establish the requested matrix.

No fake harness, fake channel, fake door, provider charge, push, merge, PR
publication, or user-session mutation was used.

## Automatic routing scope

Existing source contracts route lifecycle results to the parent, commit notices
to the parent and explicit subscribers with (lane, subscriber, head)
deduplication, and PR notices by URL and subscriber. Existing focused source
coverage and prior commit cases provide evidence for those paths. The native
lane receipt matrix and explicit subscriber integration evidence remain to be
run after the launch path passes.
