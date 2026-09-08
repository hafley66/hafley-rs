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

Current next actions: run incident regressions before fixes; consolidate route
registration; isolate trail storage without changing HOME/CODEX_HOME; build only
affected packages; test generated wrapper against authenticated Codex; complete
feature and harness inventories; commit coherent changes and precise receipts.

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
0.153.4. The current wrapper still binds a thread once using cwd/time candidates;
later new-session transitions, same-cwd concurrency and process-local binding
remain unverified and require implementation work.
