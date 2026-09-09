# Lifecycle consolidation finish receipt

Date: 2026-09-09 EDT

Task receipt root: `/private/tmp/boop-sol-finish.O0YhhM`

Isolated Cargo target: `/private/tmp/boop-sol-finish-target.O0YhhM`

Starting source: `64381691b4de4cfb16a9a44316e82cafd45e1d6c`

Final implementation and live-test source: `2f5410ac3eb4a2b9714d72e3a31a80de625e7bfc`

`live-full-2/git-head.txt` contains that full hash. `live-full-2/git-status.txt`
is zero bytes, recording a clean worktree before the final live matrix. Each
matrix and launch JSON also records build `0.0.10 (2f5410a)` and the copied
test binary's SHA-256.

## Implemented

### Authoritative identity and topology

`Harness` now owns a shared `SessionTopology` classification for root,
provider-native child, and Boop-wrapped child sessions. Deterministic fixtures
cover the three classifications and an unparented child fixture which must not
inherit an unrelated root.

Automatic route binding no longer chooses a transcript because it is newest or
because it shares a cwd. Native control binds from exact process, pane, route,
session, or provider control-plane evidence. `boop-proc` delegates the decision
to `LiveSessions::live_session_for_route`; an unresolved route remains unbound.

Claude, Codex, OpenCode, and ccz behavior remains behind the central harness and
Door traits. ccz uses Claude-compatible mechanics but retains `ccz` as the
launched executable and live matrix entry.

### Queued delivery lifecycle

The accepted-delivery ledger remains the retry boundary. New deterministic
receipts exercise these state transitions:

1. A door delivery while busy is held for a turn boundary.
2. A door queue survives closing and reopening the store.
3. An ACPX prompt failure leaves the envelope pending.
4. A new process and reopened store retry the pending ACPX envelope.
5. Local acceptance suppresses later drain attempts.
6. Parent completion stays pending until the parent transport accepts it.

The live matrix exercises idle acceptance, accepted retry suppression, busy
holding, stale-route holding across frontend exit, delivery after resume, and
child completion to the registered parent for all four harness entries.

No queue algorithm was replaced in this finish pass. The existing durable
ledger and retry implementation was sealed with restart receipts, while the
route lookup which feeds it was changed to require authoritative evidence.

### OpenCode selected session, settings, and backend

The OpenCode Door now:

- subscribes to the route-owned server's `/event?directory=...` stream;
- parses legacy `properties` and durable `data` event payloads;
- observes `tui.session.select` and durable
  `session.next.model.switched` events;
- seeds a fresh route from the exact server configuration;
- seeds a resumed route from the exact session's `/api/session/{id}/history`,
  so the persisted model and effort outrank the server default;
- posts the installed model API's required
  `{model:{id,providerID,variant}}` payload and reads it back from exact session
  history;
- creates a fresh session on the route-owned backend and selects that exact ID
  through `/tui/select-session?directory=...` for the adapter clear operation;
- resumes the selected cleared session and retains the Boop trace and parent;
- declares and exercises a separately owned backend.

Installed OpenCode 1.18.25 implements an internally typed `/clear` by navigating
the TUI-local route to home. That action creates no session and emits no server
event. The upstream OpenCode issue
[`#31051`](https://github.com/anomalyco/opencode/issues/31051) records the same
missing internal navigation event. Passive immediate observation of a
separately typed literal `/clear` therefore remains unavailable in this
installed version. Boop's adapter clear operation uses the installed exact
create/select control plane, and the final live receipt proves selection,
delivery, exit, and resume on the returned session without cwd transcript
inference.

### Central lifecycle matrix

The live driver reads backend and settings support from the central adapter
capabilities and invokes settings and clear through the Door trait. A completed
wrong model response during the parent readiness handshake may be retried up to
two times; an in-flight turn is never interrupted or duplicated by the retry.

## Deterministic verification

Command:

```text
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/private/tmp/boop-sol-finish-target.O0YhhM \
  bash crates/boop/scripts/0_regression_gate.sh deterministic
```

Result: PASS in 50 seconds from receipt-log creation to final write.

- Locked default package gate: 837 passed, 0 failed, 8 ignored.
- Locked `boop` `dl6` gate: 241 passed, 0 failed, 2 ignored.
- `boop --no-default-features` check: PASS.
- Total test executions: 1,078 passed, 0 failed, 10 ignored.
- Receipt: `/private/tmp/boop-sol-finish.O0YhhM/deterministic-final-2.log`.

Focused current results before the gate:

- OpenCode Door: 14 passed, 0 failed.
- Codex Door: 12 passed, 0 failed.
- Central topology fixtures: 2 passed, 0 failed.
- Exact Claude session reader: 1 passed, 0 failed.
- Exact route-evidence binding: PASS.
- ACPX failure, store reopen, retry, and accepted suppression: PASS.
- Lifecycle live driver compile: PASS.

The first uncapped central run completed the workspace suites but its `dl6`
link was terminated by SIGTERM without a compiler diagnostic. The bounded
`CARGO_BUILD_JOBS=2` runs passed. Red and subsequent green logs are retained as
`deterministic-after-fix.log`, `deterministic-jobs2.log`,
`deterministic-final.log`, and `deterministic-final-2.log`.

## Authenticated live verification

Installed executables:

- Claude: `2.1.265 (Claude Code)`
- Codex: `codex-cli 0.153.4`
- OpenCode: `1.18.25`
- ccz: `2.1.266 (Claude Code)`

The affected OpenCode row passed 18 PASS, 0 FAIL, 0 BLOCKED, 0 UNSUPPORTED in
255.69 seconds:

`/private/tmp/boop-sol-finish.O0YhhM/live-opencode-7/opencode-31921/matrix.json`

The final complete four-harness gate took 834 seconds by receipt-log timestamps.
Harness test timings reported by Cargo were:

| Harness | PASS | FAIL | BLOCKED | UNSUPPORTED | Time |
| --- | ---: | ---: | ---: | ---: | ---: |
| Claude | 15 | 0 | 0 | 3 | 173.29 s |
| Codex | 18 | 0 | 0 | 0 | 177.36 s |
| OpenCode | 18 | 0 | 0 | 0 | 259.02 s |
| ccz | 15 | 0 | 0 | 3 | 222.03 s |

Exact scenario matrix:

| Scenario | Claude | Codex | OpenCode | ccz |
| --- | --- | --- | --- | --- |
| `native_executable` | PASS | PASS | PASS | PASS |
| `fresh_wrapper_identity` | PASS | PASS | PASS | PASS |
| `idle_receipt_and_accepted_retry` | PASS | PASS | PASS | PASS |
| `busy_receipt` | PASS | PASS | PASS | PASS |
| `exit_and_resume_across_processes` | PASS | PASS | PASS | PASS |
| `stale_route` | PASS | PASS | PASS | PASS |
| `compact` | PASS | PASS | PASS | PASS |
| `resume_after_compact` | PASS | PASS | PASS | PASS |
| `model_and_effort_change` | UNSUPPORTED | PASS | PASS | UNSUPPORTED |
| `settings_across_resume` | UNSUPPORTED | PASS | PASS | UNSUPPORTED |
| `abnormal_exit` | PASS | PASS | PASS | PASS |
| `backend_restart` | UNSUPPORTED | PASS | PASS | UNSUPPORTED |
| `clear_new_session` | PASS | PASS | PASS | PASS |
| `resume_after_clear` | PASS | PASS | PASS | PASS |
| `concurrent_session_isolation` | PASS | PASS | PASS | PASS |
| `child_completion_parent_receipt` | PASS | PASS | PASS | PASS |
| `concurrent_process_cleanup` | PASS | PASS | PASS | PASS |
| `process_cleanup` | PASS | PASS | PASS | PASS |

Final matrix receipts:

- Claude:
  `/private/tmp/boop-sol-finish.O0YhhM/live-full-2/claude-53179/matrix.json`
- Codex:
  `/private/tmp/boop-sol-finish.O0YhhM/live-full-2/codex-63756/matrix.json`
- OpenCode:
  `/private/tmp/boop-sol-finish.O0YhhM/live-full-2/opencode-80701/matrix.json`
- ccz:
  `/private/tmp/boop-sol-finish.O0YhhM/live-full-2/ccz-99109/matrix.json`
- Combined command output:
  `/private/tmp/boop-sol-finish.O0YhhM/live-full-2.log`

Every final `process_cleanup` receipt reports an empty `surviving_pids` array.

OpenCode's final settings receipt records
`zai-coding-plan/glm-5.3-flash`, effort `high`, after frontend resume and after
backend restart. Its clear receipt records transition from
`ses_f7c77c7dbffeKGAVj0mxcjVRGf` to
`ses_f7c7502dbffexbG2wEw5sArbGR`, retaining trace
`trace-ses_f7c77c7dbffeKGAVj0mxcjVRGf`. Resume after clear passed on the new
session.

## Red receipts retained

The task-owned root retains failures used to drive the changes:

- `live-opencode/opencode-998/matrix.json`: fresh OpenCode route lacked model
  observation.
- `live-opencode-2/opencode-45238/matrix.json`: unwrapped model control payload
  received HTTP 400.
- `live-opencode-3/opencode-47894/matrix.json`: resume replaced the durable
  model with the server default.
- `live-opencode-4/opencode-28101/matrix.json`: literal internal `/clear`
  produced no observable selected-session event.
- `live-full/opencode-33410/matrix.json`: one parent readiness turn repeated the
  preceding isolation answer. Its exact task session contains the new user
  prompt, proving the route did not cross sessions.
- `live-opencode-6/opencode-6128/matrix.json`: GLM-5.3-Flash named the new
  resume token in reasoning but emitted the preceding prompt and answer. The
  route and exact cleared session were correct. The subsequent run changed the
  base/change model order and passed.

The installed OpenCode API schema captured from the isolated loopback probe is
`/private/tmp/boop-sol-finish.O0YhhM/opencode-openapi.json`.

## Unsupported behavior

Claude and ccz model/effort changes and persistence checks are deterministically
UNSUPPORTED. The installed controls require user-scoped Claude configuration.
The implementation and live gate do not read, copy, print, modify, or attempt
to restore `~/.claude/settings.json` or other Claude settings/secrets.

Claude and ccz separately owned backend restart is UNSUPPORTED. Their native
launch plans execute the frontend directly and expose no adapter-owned backend
process. Frontend abnormal exit and explicit process resume both pass.

No final scenario is BLOCKED.

## Still unproven

The deterministic and live receipts prove retry before remote acceptance and
suppression after the local accepted-delivery transition is durable. They do
not prove arbitrary exactly-once delivery if a process exits after a remote
harness accepts a request but before Boop commits that acceptance to its local
ledger. That crash window may retry the request.

Passive selected-session observation for an independently typed OpenCode 1.18.25
literal `/clear` remains unavailable because the installed TUI publishes no
route-change event and creates no session until a subsequent action. The
adapter-owned exact create/select clear operation is implemented and live
tested.

## Safety and CI coverage

- `/Users/chrishafley/projects/hafley-rs/AGENTS.md` was absent. The repository
  instructions supplied with the task were applied, including the formatter
  and non-interactive command rules.
- No global install, push, merge, real user configuration write, or unrelated
  worktree mutation was performed.
- Claude and ccz live fixtures expose only Bash and use strict MCP
  configuration. Their completion command addresses only the task-owned Boop
  binary, database, route, and tmux session.
- Concurrent same-cwd isolation passes for all four harnesses without choosing
  a latest transcript.
- CI coverage changed by adding deterministic topology, exact binding, durable
  queue transition, OpenCode event/settings/control-plane, and lifecycle
  capability fixtures. The authenticated matrix remains ignored by default and
  opt-in through the central live gate.

## Commits

1. `6f00a98` `boop: require authoritative lifecycle identity`
2. `ed71fce` `style: apply workspace rustfmt`
3. `2c0fcbd` `boop: observe OpenCode selected sessions`
4. `0bbf16a` `boop: centralize native lifecycle controls`
5. `b343830` `test: complete lifecycle capability fixtures`
6. `1e0c1db` `boop: seed OpenCode route settings`
7. `af20387` `fix: wrap OpenCode model control payload`
8. `b4ebf44` `boop: preserve OpenCode settings on resume`
9. `119115d` `boop: select exact OpenCode session on clear`
10. `2f5410a` `test: retry completed parent readiness turns`

`ed71fce` contains workspace formatter output produced by the required
repository-wide `cargo fmt`; it has no intended behavior change.
