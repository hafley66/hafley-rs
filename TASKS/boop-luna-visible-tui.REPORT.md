# Boop visible lane TUI receipt

Status: blocked

## Revision

- Base: 2376a81
- Worktree: fix/luna-visible-tui-20260912
- Code edits: none
- Report file: TASKS/boop-luna-visible-tui.REPORT.md

## Findings

Harness::spawn creates one detached tmux session whose only pane runs
boop beep lane run. run_lane_supervisor then opens an ACP or direct
stream-json child with piped stdio. The child owns the model conversation but
has no terminal, so the registered lane target is necessarily the supervisor
log pane.

The historical pre-refactor TuiChannel created a second harness window,
drove that TUI through tmux, and swapped it to window 0. The current
boop-acp extraction removed that channel. boop-mux still exposes the
window operations, but no current lane path uses them. Repointing
agent_route.tmux to a guessed child target would create a competing
conversation or an unowned pane and would break supervisor identity,
resume, retirement, and input delivery.

Required implementation dependency: restore a terminal-backed
LaneChannel in boop-acp or add a lane-native attach mode to each harness
door. The supervisor must publish the selected window-0 target before route
consumers open it. The four harness adapters need explicit profiles and
resume behavior. A regression suite must then assert pane count, window 0,
same conversation identity, input delivery, normal lane creation, and fork
creation.

## Startup reliability

The dispatched startup attempt ran just boop-start in a fresh external
target and exceeded Boop's 120-second SPAWN_CHILD_TIMEOUT; Boop killed the
process group and the parent retried with --no-start. The existing warmup
tests pass for no recipe, shared-target reuse, dry-run reporting, and
preamble recording. The timeout remains a cold-build launch blocker because
the current bounded warmup treats a slow build as a failed spawn.

## Validation

Passed:

- cargo test --locked -p boop --test main boop_start_warm -- --nocapture --test-threads=1: 5 passed.
- cargo test --locked -p boop --lib lane_subscribers -- --nocapture --test-threads=1: 0 matching tests, command passed.
- Codex and OpenCode cases in tui_sigint_e2e passed: 2 passed, 1 Claude failure.
- Commit push Codex, OpenCode, and Kimi cases passed: 3 passed, 1 Claude failure.
- Lifecycle held-row Codex and OpenCode cases passed before the suite continued into existing failures.

Failed or unavailable in the current environment:

- tui_sigint_e2e: Claude executable absent from the isolated test HOME.
- commit_push_e2e: Claude coordinator readiness absent; 3 passed, 1 failed.
- pr_push_e2e: Claude executable absent; Codex/OpenCode supervisor and ingest assertions failed in the current baseline.
- lane_lifecycle_e2e: Claude executable absent; Codex/OpenCode progressed, with the full suite still failing.
- Ignored live harness: Claude executable absent.

No fake harness, fake channel, fake door, provider, push, merge, PR publication,
or user-session mutation was used.

## Automatic routing observed

The source contracts currently cover parent result rows, explicit commit
subscribers, commit deduplication by (lane, subscriber, head), and PR notice
deduplication by URL and subscriber. The focused routing unit coverage and the
passing Codex/OpenCode/Kimi commit cases provide evidence for those paths.
The requested same-harness live TUI matrix and subscriber receipt matrix could
not be established because the lane TUI path is absent and Claude is
unavailable.

Boop-Ask: restore or specify the lane TUI attachment contract after the
boop-acp extraction, then rerun the required real-TUI matrix.
