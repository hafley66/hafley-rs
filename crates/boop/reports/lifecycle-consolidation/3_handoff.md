# Task state and join instructions

## Current checkpoint

Implementation head: `6230786`. The full deterministic gate passed in
`254_central-deterministic.log`. The central four-harness live command completed
in `256_four-harness-live.log`: Codex passed all 18 rows in 183 seconds.
Claude/ccz passed supported lifecycle rows with the two settings rows BLOCKED;
their separate-backend operation is UNSUPPORTED. OpenCode passed other rows,
failed clear binding, and has settings/resume-after-clear BLOCKED. Exact results
and paths are in `263_four-harness-results.json` and report 2.

Worktree:
`/Users/chrishafley/projects/hafley-rs/.boop-worktrees/refactor/boop-lifecycle-consolidation`.
Branch: `refactor/boop-lifecycle-consolidation`, base `66cbe8e`.
No merge, push or global install. Main checkout and other lanes remain user-owned.

Dedicated target: `/private/tmp/boop-lifecycle-consolidation-target`.
Raw receipts: `/private/tmp/boop-lifecycle-consolidation-proof-01a08191`.
Preserve the report edits and any active test invocation when resuming this task.

### Continuation after the effort correction

The full live invocation has ended and its fixtures have been cleaned up. The
scoped restart helper is `266_restart_audit.py`, with planned command in `261`.
Its result will be `266_audit-relaunch-result.json`. Before further edits, check
that the new native turn actually records Astra/max and that the lane is live.
Then continue the original brief: resolve OpenCode settings controls and its
clear/new observer through the existing adapter/protocol boundary, and investigate
an isolated authenticated Claude settings path. Do not rerun already passing
scenarios without a source change or unresolved assertion that requires it.
The Claude restoration question remains pending; do not guess the old keys.

## Required corrections and remaining gaps

1. **Audit effort does not meet the mandate.** Current native thread
   `01a08191-263d-7971-b11b-10b44d4bbd09` reports `gpt-6-astra`, effort `low`
   in `259_audit-model.json`. The actual pane launch command explicitly contains
   `boop beep lane run ... --model 'gpt-6-astra' --effort 'low'`.
   The supervisor exposes no verified running-session effort control. The lane
   must be relaunched with `max`; no correction or retroactive max assurance is
   claimed. Earlier max evidence belongs to the earlier process. Commit footers
   written during this resumed turn said max; receipt `259` contradicts that
   attribution. The native record is authoritative. A scoped relaunch plan in
   `261_audit-relaunch-plan.json` changes only the task lane's effort argument;
   it has not yet executed. Verify max in the next native turn before editing.
2. **Claude defaults restoration is pending.** Trial `201`, test
   `shared-live/claude-10568`, used native `/model claude-sonnet-4-6` and
   `/effort high`. Claude persisted these to `~/.claude/settings.json`, violating
   the no-user-config-mutation requirement. Current keys are
   `model: "claude-sonnet-4-6"`, `effortLevel: "high"`. The August 20 backup
   cannot establish their prior values. A user question is pending for those
   exact two keys; restore only those keys when answered. Do not infer values
   from an earlier native model or overwrite other settings.
3. **Claude/ccz settings E2E remains BLOCKED.** The adapter disables controls
   known to persist user defaults. A verified process-only control or isolated
   authenticated settings store is required. No credentials were printed,
   copied or replaced. Functional settings results from `201` are not passing
   isolated assurance.
4. **ccz isolation incident.** In `241`, test `ccz-54609`, the model ignored a
   no-tools nonce prompt and invoked native `ListAgents` and `SendMessage`.
   It addressed unrelated `gothic-cf` with `ACK_BOOP_E2E_ccz_54609_busy`.
   This is an isolation FAIL. No follow-up, transcript inspection or cleanup
   touched the unrelated session. Claude/ccz now launch with
   `--tools Bash --strict-mcp-config`. Restricted run `ccz-76047` verified
   busy delivery and live parent completion; `255_ccz-tool-isolation.json`
   found only Bash in its recorded native threads.
5. **OpenCode clear/new binding remains FAIL.** In `241`, native `/clear`
   returned the TUI to its start screen while Boop retained the old thread ID.
   The installed API/source has a navigation-request event
   `tui.session.select`; no verified selected-session observation has been
   established for local clear/new navigation. Do not send to the stale route
   or infer identity from the latest same-cwd transcript. The shared driver
   records failed clear, explicitly exits/resumes its prior test conversation,
   and continues independent assertions.
6. **OpenCode model/effort control remains BLOCKED.** Explicit launch-model
   configuration and observed native model are verified. Its existing HTTP
   session model API is captured in
   `shared-live/opencode-85957/native-api.json`; in-session settings and variant
   changes still need adapter operations and native execution assertions.
7. **Remote acceptance crash window remains unverified.** Durable acceptance
   prevents subsequent retries. A crash after remote acceptance but before the
   local receipt remains ambiguous; no tested native idempotency key closes it.
8. **Compatibility obligations remain visible.** Legacy sweep uses CASS and
   cannot prove a session within a shared-database source path without a session
   identifier. Public summary spellings share one implementation. Typed DB views
   retain distinct projections. Report 0 records these boundaries and unverified
   option combinations; supported features were not removed to lower counts.

## Central commands and gates

From this worktree:

```bash
CARGO_TARGET_DIR=/private/tmp/boop-lifecycle-consolidation-target just boop-check deterministic

CARGO_TARGET_DIR=/private/tmp/boop-lifecycle-consolidation-target \
BOOP_E2E_ROOT=/private/tmp/boop-lifecycle-consolidation-proof-01a08191 \
BOOP_E2E_OPENCODE_MODEL=zai-coding-plan/glm-5.3-flash \
just boop-check live
```

Each live invocation creates a fresh per-process diagnostic directory. The
OpenCode model above was explicitly selected from the installed provider's
advertised models in `236_opencode-models.txt`; provider rejection never selects
another model silently. The script can select entries with
`bash crates/boop/scripts/0_regression_gate.sh live codex`.

`254` passed the seven affected packages, boop dl6 tests and no-default-features
check. Boop default integration: 127 passed, 1 ignored. dl6 integration:
130 passed, 2 ignored. Boop binary: 107 passed. Harness: 179 passed, 1 ignored.
The six ACP authenticated/machine tests, one native Claude door test and
authenticated integration tests remain explicitly ignored by deterministic mode.
Their fixtures are not presented as live proof.

Gate `234` had a lane-carcass timeout; its focused 6-test rerun `237` passed.
Complete gates `242` and `254` subsequently passed. Before/after reproductions
and historical gates remain in the raw receipt manifest.

## Filesystem map

- `crates/boop/scripts/0_regression_gate.sh`: central deterministic/live entry.
- `justfile`, `.github/workflows/ci.yml`: discoverable command and deterministic CI.
- `crates/boop/tests/4_lifecycle_gate.rs`: shared lifecycle scenarios/assertions,
  native adapter operations, frozen executable, private tmux, receipts and cleanup.
- `crates/boop/tests/1_harness_boundaries.rs`: syn architectural guard and inventory.
- `crates/boop/src/cli/control.rs`: native wrapper ownership, typed observations,
  binding, restart and existing pending-mail drain.
- `crates/boop-harness/src/harness/`, `src/door/`: harness behavior and transports.
- `crates/boop-acp/src/channel/`: existing ACP lane and ACPX channels.
- `crates/boop-proc/src/deliver.rs`: canonical delivery admission and retry path.
- `crates/boop-store/src/bus.rs`: route persistence and native-owned field update.
- `crates/boop-proc/src/config.rs`: complete preset selection used by lane creation.
- Reports `0` through `5`: feature inventory, boundaries, live receipts, this
  handoff, receipt hashes and per-symbol AST inventory.

## Commits

In implementation order:

- `29fad26`: preserve route kind and omitted metadata when binding panes.
- `2376ef5`: isolate trail storage and coalesce observations.
- `b815481`: observe actual wrapped Codex connections.
- `501d44c`: retain process observations and release owned resources.
- `f4480d1`: isolate readers and avoid telemetry mailbox writes.
- `8b6097d`: move behavioral dispatch into existing adapters and add guard.
- `d2cc152`: preserve native CLI passthrough and observed resume settings.
- `bb5445b`: serialize retries and retain held completion outboxes.
- `8da1197`: restart owned backends with observed settings.
- `55b9e28`: central gate and native adapter bindings.
- `fa8dfc6`: native settings and bound identity priority.
- `e19782c`: Claude queued receipt reader and caller favorites.
- `4c94a64`: preserve parent registration during native observations.
- `9eff9fd`: shared authenticated lifecycle driver.
- `c906599`: busy OpenCode holding, bounded sweep and completion approval.
- `6230786`: canonical complete preset selection and fixture restrictions.

## Join, resume and cleanup

Audit lane `refactor-boop-lifecycle-consolidation` is live in pane `%1939`,
pane PID `88860` at the latest observation. Native thread remains
`01a08191-263d-7971-b11b-10b44d4bbd09`. Its parent is
`sprefa-ivm-extract-parent`. The effort correction above is still required.

Outside tmux:
`tmux attach-session -t refactor-boop-lifecycle-consolidation`.

Inside tmux:
`tmux switch-client -t refactor-boop-lifecycle-consolidation`.

Test wrapper resume commands and executable hashes are in each numbered
`*_launch.json` and `*_launch.bash`. These scripts include isolated Boop
storage and exact native session arguments. Completed fixtures have been stopped;
create a fresh test invocation instead of treating an old pane as live.

`258_old-test-pane-cleanup.json` verifies removal of four dead manual-test
sessions after checking their exact pane IDs, dead state and task-owned launch
commands. No parent/audit pane was removed. Raw test receipts and native
transcripts remain available; complete user transcripts are not committed.
`265_test-workspace-cleanup.json` records removal of 22 empty, completed fixture
working directories. No recursive deletion was used for that cleanup.

## Protected parent delivery

Parent `sprefa-ivm-extract-parent`, native thread
`01a067ea-5289-7092-9d52-3588c1af9555`, pane `%384`, session `projects-4`,
was not repaired or used as a fixture. Milestones still return
`held-for-turn-boundary (lane supervisor)`. Latest recorded IDs:
`m-7dbea796` (`229`) and `m-3b67435b` (`246`). No actual parent receipt is
claimed. A repair proposal must preserve the real parent kind and native thread;
mail insertion or an accepted flag is not evidence of its live receipt.
