# Task state and join instructions

Work in progress. Baseline commit: `66cbe8e`. No merge, push or global install.
Dedicated target directory: `/private/tmp/boop-lifecycle-consolidation-target`.
Raw receipts: `/private/tmp/boop-lifecycle-consolidation-proof-01a08191`.

The active audit lane remains `refactor-boop-lifecycle-consolidation`, pane
`%1811`, Codex thread `01a08191-263d-7971-b11b-10b44d4bbd09`.
Parent edge: `sprefa-ivm-extract-parent`.

Join from outside tmux: `tmux attach-session -t refactor-boop-lifecycle-consolidation`.
Join from inside tmux: `tmux switch-client -t refactor-boop-lifecycle-consolidation`.
Verified session and pane: `refactor-boop-lifecycle-consolidation %1811`.
The installed CLI has no `beep lane join` command. The audit lane must remain available while
implementation and live cases are pending.

Milestone push `m-6e05b74e` at 2026-09-08T15:12:08Z returned
`held-for-turn-boundary (lane supervisor)` for the parent. No parent transcript
receipt has been established. The user's parent route and pane were not repaired
or used as a test fixture.

Completed commits:

- `29fad26 fix(boop): preserve route ownership when binding existing panes`
- `2376ef5 fix(boop-store): isolate trails and coalesce lifecycle observations`
- `b815481 feat(boop): observe wrapped Codex lifecycle through owned connections`

Post-checkpoint fixes: transcript-only refresh preserves explicit PID/pane
observations (failing reproduction `75_projector-pid-before.log`, passing
`76_projector-pid-after.log`). Native launch resources now own both frontend and
backend cleanup. Exit releases the route socket and liveness only while still
owned; a later resume is preserved. Owned unbound routes cannot select a thread
by cwd. Control 10 passed (`78_native-cleanup-regressions.log`); process-drop
regression passed (`79_plan-drop.log`). Integration fixture migration remains next.

Current implementation work owns a separate Codex app-server process group and
private socket per wrapped TUI. Observed protocol events carry thread/model/effort
into the existing route and session tables. Storage overrides now include trail
paths. The latest worktree binary built successfully (`28_live-build.log`).

The native adapter now observes the actual TUI websocket connection, forwarding
both directions with Tokio/Tungstenite. It correlates start/resume/fork responses
and receives the TUI's settings/closure notifications. This replaced an observer
client that missed resumed threads and later settings. Only selected persisted
root threads bind routes; global thread-started/ephemeral guardian events do not.
The generated Bash entries share `boop tui` inside and outside tmux.

Current authenticated evidence: initial thread
`01a081bf-5d5a-7f23-a33d-81d349d9d56f` received idle and busy nonces, changed
Luna/low to Terra/medium/high, compacted, exited and resumed in another process.
The held resume nonce then arrived automatically. `/clear` created
`01a081d7-85ca-7ca1-a8ce-f3ce84dadc50`; route, parent and Boop trace were maintained.
The new thread received a nonce and was resumed in another process. Clear used
Codex's default Astra/xhigh settings; a supported per-thread API change selected
Luna/low before the bounded post-clear prompt. No user config was changed.

Same-cwd concurrent proof uses route `codex-1828` in pane `%1828` and pane-less
route `codex-process-51263` in a real PTY held by test pane `%1831`. Its independent
thread is `01a081e4-1c58-7f83-9e4a-24d2d414bdf9`. Each received one distinct nonce
and one matching answer, with zero occurrences in the other transcript.
See `67_concurrency-verdict.json`, its per-side receipts and actual OS process
trees. Both trials stopped at 16:48:15 UTC: A exited normally; B received a
verified TERM at its wrapper PID. Both owned sockets disappeared. Dead test tmux
cells remain for scoped cleanup. The audit lane `%1811` stays open.

Known failures retained: first guardian misbinding and failed nonce queues;
pre-fix timeout orphan (verified test-only process group 5943 cleaned up);
missing resume broadcast; synchronous forwarding timeout. TERM/HUP handling and
owned backend cleanup are implemented. SIGKILL/restart cases remain open.

Latest gates: store 176 passed (`62_store-suite.log`); harness 170 passed, one
ignored (`66_harness-suite.log`); Codex adapter 11 passed (`47_codex-adapter-tests.log`);
native control 9 passed (`58_control-after.log`); shell wrapper 1 passed covering
five entries in both pane environments (`34_wrapper-after.log`). Proc: 159 passed
(`68_proc-suite.log`); CLI unit tests: 107 passed (`70_cli-unit-suite.log`). CLI
integration suite: **105 passed, 13 failed** (`72_cli-integration-suite.log`).
Failures: three lane-carcass trail-path assertions, lane-retire/revive trail path,
spawn-identity trail path, three sync-convoy fixtures and sync-discovery inheriting
BOOP_NO_SYNC, three tell fixtures inheriting BOOP_PARENT, and a source-scan false
positive for `trail.rs`. Tell messages stayed in fixture databases with no route
for the inherited parent name. Fix fixture environment isolation and migrate old
home-based trail expectations before rerunning. Existing fixtures still override
HOME; migrate these to Boop/harness reader injection points under the user's
instruction to leave HOME and CODEX_HOME intact. Current live binary
SHA-256: `3ef0dc3101e49ce0526948d149ce7ffc1dacfc975591371a0c24f599f537e99f`.
`m-322f985c` is held on test route `codex-1828` after its socket disappeared. A
later explicit resume of cleared thread `01a081d7-85ca-7ca1-a8ce-f3ce84dadc50` in
test pane `%1828` can prove automatic recovery of that held message.

Next actions:

1. Finish the native-wrapper commit and save hashed receipt manifests.
2. Fix the remaining competing liveness writer: transcript projection currently
   erases observed PID/pane/status. `67_concurrency-verdict.json` proves the null
   PID/pane cache while real processes are running. `ident::sync_session_with`
   calls `record_status` with transcript-derived state and no PID. Preserve
   explicit live ownership; add a refresh regression.
3. Make frontend cleanup belong to the native launch resource on every error
   path. Clear route transport/status on exit, prevent owned unbound routes from
   cwd discovery, and prove TERM/stale/restart/reattach. SIGKILL requires special
   attention: an earlier killed wrapper orphaned its backend.
4. Complete retry/concurrent delivery admission coverage and child completion /
   parent notification proof. Ordinary nonce receipt counts pass; arbitrary
   concurrent retries of one message have not been proved.
5. Complete inventories and harness dispatch guard. Candidate behavioral branch:
   `cli/job.rs` Claude native-worktree discovery. Static harness metadata must be
   classified separately. Retired `boop-acp/channel/codex.rs`, duplicate discovery,
   fallback dispatch and direct mail paths remain under review.
6. Finish opt-in authenticated E2E runner, all affected-package gates and reports.

Milestone `m-1d4a403e` at 16:24:34 UTC was again held for `lane supervisor` on
the user's parent (`50_parent-milestone.txt`). No parent transcript receipt is
claimed. A repair has not been applied to the user's parent route.

Raw driver files: `0_launch_baseline.bash` evaluates actual generated shell-init;
`4_resume_live.bash` does the same for explicit resume; `1_capture_live.py` records
route, actual thread/turn data, trace, attributes, OS process tree and delivery
transitions. The Node helpers use an already cached `ws` library without installs.
These prototypes remain outside Git; migrate them into a portable opt-in runner.

Audit thread metadata verifies `gpt-6-astra`, effort `max`. Initial incident
regressions: 5 passed, 3 failed, 109 filtered; raw `2_regression-before.log`.

Registration cluster gates: `cargo test -p boop --test main -- registry_kinds::`
(8 passed), `cargo test -p boop --bin boop -- cli::me::` (3 passed),
`cargo test -p boop --test main -- coordinator_ping::` (4 passed).
Complete affected-package gates remain pending until implementation finishes.

Removed duplicate adoption/pane lookup and route serialization paths. Existing
lane patch now preserves kind and metadata, supports `%pane`, and errors on a
missing target. Agent register accepts `--session-id` and `--tmux` and preserves
omitted fields. New pane attachments register as coordinators.

Installed Codex supports `app-server --listen unix://PATH` and generated its
protocol schemas under the raw receipt root. The shared daemon reports version
0.153.4. The uncommitted Codex wrapper observes its owned backend. Other adapters
still use existing registry discovery; their identity paths remain under review.
Raw receipt `26_loaded-threads.json` contains the task-owned initial answer;
`22_idle-send.txt` and `23_idle-send.txt` retain the failed queue deliveries.
