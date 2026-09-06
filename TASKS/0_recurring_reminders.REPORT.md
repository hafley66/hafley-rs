# Recurring reminders receipt, 2026-09-06

Lane: `feature/boop-reminders-instant`, base `2ea68efff2a5afb15c3f229d4c05e837a080ef76`.
Execution preset verified from the lane spawn receipt: `gpt-6-astra`, effort `high`.
No delegated workers. One compiler job, isolated Cargo targets, sequential browsers.

## Interface and persistence

`boop beep remind add NAME ROUTE BODY --every 30m --until EPOCH_OR_RFC3339`
persists a schedule for an existing explicit route. `run`, `list`, and `cancel`
share `--mail-dir`. First delivery follows one interval. Expiry is exclusive.
No recursive agents, automatic lane revival, or background installation.

The existing mailbox, routes, delivery ladder, door budget, harness idle notifications,
and supervisor turn receipts are reused. Schema 28 adds one `agent_reminder` table.
The claim and envelope commit together. One runner lock per mailbox directory,
32 active schedules, 32 native completion observers, one active schedule per route,
and one outstanding occurrence per route bound delivery. Cancellation and a new
schedule do not release an earlier accepted or uncertain occurrence.

Missed intervals collapse. Recorded refusals retry the same message id at the
interval. A crash with an uncertain outcome never causes an automatic replay.
Admission and mailbox acknowledgment alone do not release overlap; a turn-end or
threaded recipient reply does. A missed completion notification across restart
can leave work outstanding until an explicit persisted receipt is available.
Use the foreground runner for native doors. `--once` is for separately recorded
supervisor or threaded-reply completion receipts.

Cancel and expiry suppress pending mail at read and delivery boundaries. Existing
mailbox readers see the explicit `reminder-inactive` projection marker for an
inactive unacknowledged row; the raw row keeps its NULL acknowledgment and history.
Accepted harness work cannot be recalled. In particular, an offline Codex queue
may be consumed after the schedule expires. The scheduler stops new submissions
at expiry; it cannot promise a recipient stops executing previously accepted work.
Stores containing reminder history reject destructive projection rebuilds.

Native Codex, Claude, OpenCode doors and existing supervised lanes share this API.
Kimi uses its existing supervised ACP lane. ACPX coordinator mode is explicitly
unavailable: its current queue enables `--approve-all` and can fall back to npx.
This implementation does not invoke that path or change its permission policy.

## Tests

Regression command, temporary store:

```sh
BOOP_DB=/private/tmp/boop-reminder-regression/boop.db \
CARGO_BUILD_JOBS=1 \
CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/cargo-target-reminders-instant \
cargo test -p boop-store -p boop-proc -p boop --lib --bins -- --test-threads=1
```

400 passed: Boop library 10, CLI binary 85, boop-proc 146, boop-store 159.
Log: `/private/tmp/boop-reminder-final-regression.log`.
The CLI integration module adds three tests for explicit session registration,
add/list/cancel/restart, unavailable and dead routes, runner lock, and expiry.
Store tests cover due boundaries, missed intervals, expiry, cancellation,
duplicate claims, overlap, replacement, definite-refusal retries, and restart.
Supervisor tests cover expiry/cancel during a held turn and the full completion
receipt in the custom mail store.

Adapter fixture command:

```sh
CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/cargo-target-reminders-instant \
BOOP_DB=/private/tmp/boop-reminder-regression/boop.db \
cargo test -p boop-proc reminder_adapter_receipts_and_expiry -- --test-threads=1 --nocapture
```

| Harness | Fixture message | Transitions | Evidence boundary |
|---|---|---|---|
| Codex | `m-5f326e7f` | appended, accepted-by-harness | Fake door only |
| Claude | `m-a4df4762` | appended, accepted-by-harness | Fake door only |
| OpenCode | `m-3ecc3240` | appended, accepted-by-harness | Fake door only |
| Kimi | `m-3c27c2f3` | appended, held-for-turn-boundary | Supervisor handoff only |

The separate supervisor fixture reaches appended, claimed-by-supervisor,
submitted-to-harness, accepted-by-harness, turn-ended using a bounded fake channel.
Fixture ids are minted anew on subsequent test executions.

## Live harness receipts

All reminder probes used `/private/tmp/boop-reminder-live-mail`, disposable
recipients, token-only requests, and explicit short expiry. Both schedules expired.
No work was sent to `codex-tsi`.

```sh
B=/Users/chrishafley/.cache/boop/cargo-target-reminders-instant/debug/boop
D=/private/tmp/boop-reminder-live-mail
"$B" beep agent register receipt-claude --kind coordinator --harness claude \
  --session 9c6a57ec-ee76-4d01-833e-a013f341a2e2 \
  --cwd /private/tmp/boop-reminder-live-claude --mail-dir "$D"
"$B" beep remind add claude-live receipt-claude \
  'Receipt token BOOP-CLAUDE-REMINDER-20260906. Reply with this token only.' \
  --every 4s --until 1788721794 --mail-dir "$D"
"$B" beep remind run --mail-dir "$D"
"$B" beep agent register receipt-codex --kind coordinator --harness codex \
  --session 01a07821-d695-79a2-86fe-e90ce358b9af \
  --cwd /private/tmp/boop-reminder-live-claude --mail-dir "$D"
"$B" beep remind add codex-live receipt-codex \
  'Receipt token BOOP-CODEX-REMINDER-20260906. No tools. Reply with this token only.' \
  --every 3s --until 1788721983 --mail-dir "$D"
"$B" beep remind run --mail-dir "$D"
```

| Harness | Message | Recorded delivery | Recipient read / result |
|---|---|---|---|
| Claude | `m-c6f7d04d` | appended 1788721788048; accepted 1788721788050; turn-ended 1788721792671 | Exact assistant echo `BOOP-CLAUDE-REMINDER-20260906` |
| Claude | `m-ad1d4c84` | appended 1788721793376; cooled-off 1788721793377 | No second delivery; existing repeated-body budget applied |
| Codex | `m-5ee3d93d` | appended 1788721978376; accepted 1788721978443 | Queued while offline; exact assistant echo after bounded resume at 2026-09-06T19:16:02.366Z |
| Codex connected | `m-f37e14c4` | appended 1788722994740; accepted 1788722994810; turn-ended 1788723000011 | Exact assistant echo `BOOP-CODEX-CONNECTED-20260906` at 19:29:59.970Z |
| Codex connected | `m-72ef9451` | appended, cooled-off, submitted-to-harness, cooled-off | Same id retried after definite cooldown refusal; no duplicate delivered body |
| OpenCode | none | Skipped live | No verified authorized flat-rate provider; paid OpenRouter inference prohibited by brief |
| Kimi | none | Blocked live | Existing ACP adapter auto-grants permission requests; no widened permissions authorized |

Claude was launched with `--bg --safe-mode --restricted --tools '' --strict-mcp-config`
and a token-only prompt. The CLI ignored the requested session UUID and reported
the actual UUID recorded above. Launch cwd was `/private/tmp`; the explicit
registered session controlled delivery. Claude's built-in safeguards switched its
requested Fable model to `claude-opus-4-8`; no safeguard override was attempted.
Transcript: `~/.claude/projects/-private-tmp/9c6a57ec-ee76-4d01-833e-a013f341a2e2.jsonl`.
`claude stop 9c6a57ec` completed after the probe.

Codex startup command:

```sh
/Users/chrishafley/.local/bin/codex.native exec --ignore-user-config --ignore-rules \
  --sandbox read-only --skip-git-repo-check --model gpt-6-astra \
  -c 'model_reasoning_effort="high"' --json \
  'This is a disposable transport receipt. Do not use tools or inspect files. Reply exactly READY.'
```

The initial wrapper invocation failed before starting a session because `--remote`
is interactive-only. The native command succeeded. Its offline queue receipt did
not prove a read. A bounded native TUI resume of the same disposable thread with
`--remote unix:///Users/chrishafley/.codex/app-server-control/app-server-control.sock
--sandbox read-only --model gpt-6-astra -c 'model_reasoning_effort="high"' resume
01a07821-d695-79a2-86fe-e90ce358b9af` consumed the token, then detached with Ctrl-C.
No tools were called. Transcript:
`~/.codex/sessions/2026/09/06/rollout-2026-09-06T15-11-18-01a07821-d695-79a2-86fe-e90ce358b9af.jsonl`.
The runner had already expired, so no scheduler turn-end receipt is claimed for
this Codex delivery. The transcript task_complete is separate evidence.

A second Codex probe kept that disposable native TUI connected throughout the
schedule. It used the same registration in `/private/tmp/boop-reminder-live-connected`,
`--every 10s`, an explicit 35-second expiry, and body
`Receipt token BOOP-CODEX-CONNECTED-20260906. No tools. Reply with this token only.`
The exact argument arrays, result and expiry are recorded in
`/private/tmp/boop-codex-connected-receipt.log`. The table above records the
scheduler's actual completion notification and definite-refusal retry. The
schedule expired and the completed disposable TUI was detached afterward.

## Proposed Game3 replacement, pending coordinator cutover

No cron entry, original script, stop marker, or Game3 code was changed. The
following uses a dedicated namespace so older installed Boop readers cannot
drain its pending reminders. Register and add once after stopping the old cron;
restart only the final foreground `run` command thereafter.

```sh
B=/Users/chrishafley/.cache/boop/cargo-target-reminders-instant/debug/boop
D=/private/tmp/game3-overnight-boop-mail
export BOOP_DB="$D/boop.db"
"$B" beep agent register game3-overnight --kind coordinator --harness codex \
  --session 01a06ebd-c7d2-7ce1-b1d8-1eb9a665ddf6 \
  --cwd /Users/chrishafley/projects/hafley-rs-game-runtime --mail-dir "$D"
"$B" beep remind add game3-overnight game3-overnight \
  'Boop beep: continue the requested Game3 overnight iteration from docs/2_next.md. Use one compiler job and one browser run at a time. Test Falcon, items, stage, destructibles, replay, online play and error conditions. Keep code growth measured; commit and push tested checkpoints. Do not expand permissions. Stop this reminder on completion or user pause.' \
  --every 30m --until 2026-09-07T00:00:00-04:00 --mail-dir "$D"
"$B" beep remind run --mail-dir "$D"
```

Cancel with `"$B" beep remind cancel game3-overnight --mail-dir "$D"`, list with
`"$B" beep remind list --mail-dir "$D"`; keep the same BOOP_DB namespace.
The previous stop marker belongs to the old shell script. After cutover, use
Boop cancel. No new submission occurs at or after epoch `1788753600`.
The accepted-work limitation above also applies to this replacement.

The real `game3-overnight` route in the existing mailbox was registered through
the new supported `--session` API with the actual coordinator thread; status
message `m-d0b087b8` was accepted by its Codex door. The unrelated stale
`codex-tsi` route was preserved. This registration did not create a reminder.

## Integration boundaries

Primary Boop dirty-file overlap: `crates/boop/src/main.rs`. The primary also has
unrelated dirty identity, control, mail, tell-test and chat-log files. No primary
files were edited and no integration was attempted. Instant uses its separate
`feature/boop-reminders-instant` worktree with a sibling path dependency pointing
to this Boop worktree. Instant and optional-reason receipts are a separate checkpoint.
