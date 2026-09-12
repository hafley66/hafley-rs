# Boop visible lane TUI receipt

Status: wip

## Revision

- Base: 2376a81
- Worktree: fix/luna-visible-tui-20260912
- Checkpoints: cae3ab3, fa619fd, a5572a2, 5b22b75, 57534d8, e4b0cdf, 5de7cad, a71f454, 82d966b, pending staged-input/status fix

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
that first user message, targets the canonical lane session, and uses the door
after materialization. Turn completion now requires transcript evidence for a
new assistant message and includes text and tool count in TurnReceipt. Fresh
Claude binding checks the frontend PID against the native session registry.
Native input resolves the pane id from the declared SpawnSpec tmux target and
socket, sends text, records evidence, waits 250 ms, then sends Enter. Steering
defers while a turn is pending so supervisor reoffers the hail once. Codex
native receipts are assembled from selected-thread item and turn events because
the real llmock test disables rollout storage. Receipts retain thread and turn
ids, terminal status, assistant items, tool item counts, and item-id
deduplication. Only completed turns with assistant text satisfy startup
acknowledgment. Lane supervisor tracing uses the existing file-only writer, so
diagnostics do not interleave with the native terminal.

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
- Focused real Codex TUI lifecycle after bounded readiness: failed with `startup acknowledgment timed out after 30s` after readiness and native session binding passed. Retained scratch root: `/var/folders/z2/cwfm40fn65n176q8m227wl0r0000gn/T/boop-lifecycle-held-codex-49167`.
- Captured input evidence from the subsequent run: `/var/folders/z2/cwfm40fn65n176q8m227wl0r0000gn/T/boop-lifecycle-held-codex-58330/mail/lanes/feature-lifecycle-held-codex/native-input-evidence.json`; socket `boop-lifecycle-held-codex-58330`, target `feature-lifecycle-held-codex`, pane `%1`, input target `%1`, frontend pid `58781`, frontend still running, and the screen showed the rendered Codex composer with no submitted row. The capture is the current pane screen; `capture-pane -S -400` was used separately for scrollback inspection.
- Native Codex observer unit suite: 16 passed, 1 ignored. The real rerun after native protocol receipt parsing, pane input routing, and file-only lane tracing remains required.
- Native Codex receipt shape regression: 1 passed, proving one receipt from agent text plus function call items and no second delivery from the consumed evidence.
- Focused rerun after the native receipt parser, pane input routing, stable readiness, and file-only lane tracing: failed with the same `startup acknowledgment timed out after 30s`. Retained root: `/var/folders/z2/cwfm40fn65n176q8m227wl0r0000gn/T/boop-lifecycle-held-codex-70953`; input evidence is `/var/folders/z2/cwfm40fn65n176q8m227wl0r0000gn/T/boop-lifecycle-held-codex-70953/mail/lanes/feature-lifecycle-held-codex/native-input-evidence.json`. It records socket `boop-lifecycle-held-codex-70953`, target `feature-lifecycle-held-codex`, pane and input target `%1`, live frontend pid `71292`, and the rendered TUI with no composer text after submission. The remaining blocker is native Codex input acceptance in the frontend process.
- Focused latest A/B rerun: `pass held codex`, 1 passed, 0 failed, 0 ignored. Retained root: `/var/folders/z2/cwfm40fn65n176q8m227wl0r0000gn/T/boop-lifecycle-held-codex-9225`; evidence is `/var/folders/z2/cwfm40fn65n176q8m227wl0r0000gn/T/boop-lifecycle-held-codex-9225/mail/lanes/feature-lifecycle-held-codex/native-input-evidence.json`. It records socket `boop-lifecycle-held-codex-9225`, target `feature-lifecycle-held-codex`, pane and input target `%1`, frontend pid `10021`, no text after the first send, and the exact readiness probe visible after the separate Enter stage. The real lane completed its lifecycle and parent receipt.
- Native Codex parser suite after status and dedup changes: 18 passed, 1 ignored. Pure cases cover failed and interrupted partial text, multiple assistant items, duplicate item events, two turns, and repeated completion suppression.
- `cargo check --locked -p boop-proc -p boop-harness -p boop`: pass after lifecycle and diagnostic changes.
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
