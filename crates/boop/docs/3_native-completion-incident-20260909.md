# Native child completion incident, 2026-09-09

## Observed failure

Boop wrote a `delivered native-child-completion:… through the codex door`
status line onto a terminal owned by the Codex TUI. The line disappeared on
the next TUI redraw. At that point the parent conversation had received no
completion message. The queued message entered the parent transcript 78.979
seconds after queue acceptance.

The completion was also false: Boop interpreted a completed parent turn copied
into the child's forked history as completion of the child. The child's own
turn was subsequently interrupted and contains no `task_complete` event.

## Evidence and sequence

Times below are UTC on September 9. Local EDT is UTC minus four hours.

Parent thread: `01a087c5-432d-78b0-b351-abd863614d6b`.
Child thread: `01a087c6-18fc-7e81-b973-140dd72fccc7`.
Route: `codex-1830`.

| Time | Evidence | Event |
|---|---|---|
| 20:05:01.871 | Child JSONL, lines 1–2 | Own child metadata followed by copied parent metadata. |
| 20:05:01.871 | Child JSONL, line 10 | Copied parent `task_complete`, for parent turn `01a087c5-7a01-7102-83cb-c5c78f7afb40`. |
| 20:05:01.884 | Child JSONL, line 15 | `thread_settings_applied` explicitly names the child thread. |
| 20:05:01.893 | Child JSONL, line 16 | Child's own turn starts. |
| 20:05:07.228122 | `agent_mail`, sequence 11666 | Boop appends `native child completed`. |
| 20:05:07.627 | `agent_delivery_transition`, sequence 2 | Boop records `accepted-by-harness`, detail `door`. |
| 20:05:07.627579 | `codex-1830/supervise.log`, line 21 | Delivery callback logs the same landing. |
| 20:05:08.907 | Child JSONL, line 27 | Child's own turn aborts with reason `interrupted`. |
| 20:05:43.775 | Parent JSONL, line 72 | User pastes the terminal's delivery status. |
| 20:06:26.341 | Parent JSONL, line 103 | Parent finishes its reply, reporting that no completion had arrived. |
| 20:06:26.606 | Parent JSONL, line 109 | Actual `[boop native-child-completion:…]` message enters the conversation. |

Original local evidence paths:

- `/Users/chrishafley/.codex/sessions/2026/09/09/rollout-2026-09-09T16-04-07-01a087c5-432d-78b0-b351-abd863614d6b.jsonl`
- `/Users/chrishafley/.codex/sessions/2026/09/09/rollout-2026-09-09T16-05-01-01a087c6-18fc-7e81-b973-140dd72fccc7.jsonl`
- `/Users/chrishafley/.agent/lanes/codex-1830/supervise.log`
- `/Users/chrishafley/.agent/boop.db`, tables `agent_mail` and `agent_delivery_transition`

The investigation read these files and tables without altering session records
or delivery history. Original transcripts include unrelated private content;
regression fixtures should contain only the record shapes needed for this bug.

## Root causes

### Inherited history was attributed to the child

`crates/boop-harness/src/harness/codex.rs::native_child_events_from_lines`
treated every `session_meta` as the current child's spawn and every
`event_msg/task_complete` as that child's completion. Full-history forks can
contain another session's metadata and lifecycle events. The observed child
contained the parent's completed clarification turn before its own turn began.

Boop's projector persisted that false completion, created a stable completion
envelope, and delivered it. The later interruption did not cause the alert:
the false envelope had already been queued before the interruption record.

### The resident wrapper printed directly to the TUI terminal

`crates/boop/src/cli/control.rs::run_native_tui` invokes the native completion
projector with a callback to `cli/mail.rs::deliver_hail`. That CLI function
used `println!` for its delivery status and any inline reply.

Claude's preceding commit `1e06475` added a terminal ownership guard to
`cli::line` and stamped `BOOP_TUI_PANE` into spawned harness processes. This
callback bypassed `line`, and the wrapper running the callback did not inherit
the environment it supplied to its children. Merely switching that print to
`line` would leave the unstamped resident-wrapper path unprotected.

External writes to the same terminal are absent from the TUI's rendered state.
The next redraw can erase them, matching the reported disappearance while
typing. The delivery line itself is separate from a conversational message.

### Queue acceptance was presented as delivery to the active conversation

`crates/boop-harness/src/door/codex.rs::queue_message` executes
`codex queue --thread … --message … --remote …`. Success proves that Codex
accepted the queued message. `CodexDoor::deliver` mapped that success to
`Delivered::Injected`, which selected wording claiming delivery through the
door. The actual transcript shows consumption after the parent turn ended.

The queue accepted this message once and later consumed it. Re-enqueuing it
while waiting for a transcript record would create duplicates. Queue ownership
and conversational receipt must remain distinguishable in descriptions and
tests while retaining queue-acceptance deduplication.

## Original session resume failure

Luna inspected the separate, older sprefa thread
`01a08393-7ddc-71f2-95f8-7832e16357a9`. Its Codex 0.153.4 paginated history
projection stops at a duplicate rollout ordinal. This is independent of the
false child notification generated during the investigation.

Source:
`/Users/chrishafley/.codex/sessions/2026/09/08/rollout-2026-09-08T20-31-16-01a08393-7ddc-71f2-95f8-7832e16357a9.jsonl`.

| Source line | Ordinal | Record | Timestamp UTC |
|---|---|---|---|
| 7546 | 7545 | `event_msg/token_count` | 19:09:48.634 |
| 7547 | 7545 | `event_msg/thread_settings_applied` | 19:41:14.962 |
| 7548 | 7546 | `event_msg/task_started` | After the duplicate |

The source contains 7,736 valid JSON records and 39,610,438 bytes. An independent
record-by-record scan reproduced exactly one ordinal discontinuity at line
7547, byte offset 39,052,095. The persisted `thread_history_projection_state`
for this thread stops at that same byte offset and expects ordinal 7546.

`logs_2.sqlite` reports `expected ordinal 7546, got 7545`, first at
19:41:14.963 UTC, and again during `thread/resume` at 20:00:46 and `turn/start`
at 20:00:47. Luna counted 239 matching log rows. The first duplicate was written
by process 44379; later resume attempts ran in process 33533. The logs do not
establish the writer-side mechanism that reused the ordinal.

The history database has 2,373 item rows for this thread, with persisted
ordinals 9 through 7542 and none at or above 7543. The rollout still holds 190
records from the duplicate through 20:04:01.771 UTC. Those records are readable
from the source, even though history projection cannot advance through them.
Luna's SQLite `quick_check` passed for the state, history, and log databases.

The original rollout and Codex databases were left intact. This investigation
does not claim that the failed Codex session has been repaired by the Boop
changes. Repairing its ordinal stream or rebuilding its history projection
requires a separate validated recovery operation. Git connectivity verification
of the sprefa checkout also passed; that check does not validate every working
tree file or explain the missing temporary worktrees.

## Scope and verification

The implementation changes the following paths:

- `boop-harness/src/harness/codex.rs`: reconstruct explicit thread ownership
  from metadata and thread-tagged events, track exact turn ownership, and
  emit lifecycle facts only for the child. Incremental observation reads the
  prefix to recover ownership and emits only facts at or after its byte cursor.
  This adds prefix-reading work compared with the previous tail-only scan.
- `boop-harness/src/door/codex.rs`: classify successful `codex queue` as
  `QueuedForTurnBoundary`.
- `boop-proc/src/deliver.rs` and `boop-store/src/ident.rs`: record an accepted
  door queue as `held-for-turn-boundary` with detail `door queue`, display
  `queued`, and preserve acceptance-based retry suppression.
- `boop/src/cli/mail.rs` and `control.rs`: give the resident wrapper's
  completion callback an explicit log-only output path. Missing or unwritable
  log files do not redirect diagnostics to the terminal. Explicit CLI sends
  retain the existing `line` output behavior, including redirected stdout.
- `boop/src/cli/job.rs`: keep the recipient's turn-end watcher armed for
  accepted door queues.

The incident fixture failed against original commit `95bd8bb`: the original
parser returned two spawn events and a completion, while the expected result
was one spawn event. The raw receipt is
[`0_original_failure.txt`](../reports/native-completion-20260909/0_original_failure.txt).

An additional production-CLI replay uses an unchanged copy of the actual
interrupted child's transcript in an isolated `BOOP_READER_HOME`, database,
mailbox, configuration path, and lane directory. No parent route is registered,
so the replay sends no messages to a live conversation. The original installed
binary, `boop 0.0.10 (8da697e-dirty)`, reproduced a persisted `completed` edge
for that interrupted child. Both the fixed test binary and the installed
release produce only `spawned` for the same unchanged source. Source SHA-256:
`a7685033fd50590a5cce5f15a693b140a4a692bd3ed5bf680fc220f0aae6c465`.
[`Production replay receipt`](../reports/native-completion-20260909/2_production_replay.json)
records the original and fixed graph rows plus the installed artifact hash.

### Current test results

`bash crates/boop/scripts/0_regression_gate.sh deterministic` exited 0:

- 817 tests passed, zero failed, eight existing live tests ignored.
- `cargo check --locked -p boop --no-default-features` passed.
- Five regression tests were added; queue-state and retry expectations were
  changed. No tests were removed or newly ignored.
- A Kimi fixture test now selects the main session using the child's explicit
  parent ID. Its fixture contains two independent sessions named `main`; the
  former independent first-match selections could pair unrelated sessions.
  Kimi production behavior was unchanged.

The focused regressions cover inherited completions, interrupted children,
genuine child completion, real byte-cursor ownership recovery, captured stdout
and stderr from the resident callback, unavailable log destinations, redirected
CLI output, Codex queue arguments/classification, and retry suppression.

Receipts:

- [`Focused tests`](../reports/native-completion-20260909/1_focused_tests.txt)
- [`Complete deterministic gate`](../reports/native-completion-20260909/3_deterministic_gate.txt)
- [`Release build and installation`](../reports/native-completion-20260909/4_release_install.txt)
- [`Source and receipt SHA-256 manifest`](../reports/native-completion-20260909/5_source_manifest.json)

### Installation and activation

Built and installed from `/Users/chrishafley/projects/hafley-rs` with:

```sh
CARGO_TARGET_DIR=/private/tmp/boop-delivery-release-target-20260909 \
  CARGO_BUILD_JOBS=2 cargo install --path crates/boop --locked --force
```

Installed executable: `/Users/chrishafley/.cargo/bin/boop`.
Version: `boop 0.0.10 (2d4fc0c-dirty)`.
Installed SHA-256:
`b14f72997a77c2145f64e78c68727e5ce228cb536490e1dcd49eaa4cb81eb842`.
Its checksum matches the built release artifact. The installed executable's
isolated production replay exited 0 and emitted no false completion.

Ten changed Rust source/test files were checked byte-for-byte between the
tested checkout and the installed source checkout. The changes are present in
the main working tree, with existing Claude work preserved. No commit was
created by this fix task.

Already-running wrappers retain their previously loaded executable. Reopening
those Boop sessions loads the installed fix. This task did not terminate or
restart active user conversations. It also did not rewrite the historical false
completion row in the live Boop database or repair the separate Codex ordinal
collision documented above.
