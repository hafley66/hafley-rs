---
created: 2026-09-05
updated: 2026-09-07
type: bug
status: fixed
priority: high
epic: boop-process
---

# Lane supervisor rows take the door and cost the coordinator a turn each

## Description

## Description

Every row a lane's supervisor mints about its own run is addressed to the lane's
registered parent. When that parent is a live claude pane, the row walks the
delivery ladder, takes the DOOR rung, and arrives in the coordinator's
transcript as a peer message. Each arrival costs the coordinator a full harness
turn on a line it did not ask for and cannot act on.

The kinds that do this:

| kind | body shape |
|---|---|
| `yield` | `idle <lane> turn=N head=<sha>` |
| `yield` | `commit <lane> <a>..<b> dirty=N` |
| `head_rewound` | `head <lane> rewound: <b> does not descend from <a>` |
| `result` | `lane <lane> done rc=N (<detail>)` |
| `exited_without_completion` | `lane <lane> exited_without_completion: <why>` |
| `retrying`, `retry_budget_exhausted`, `open_failed` | one line each |

One afternoon of six lanes put roughly thirty of these into `claude-498`'s
transcript. Chris: "is there a way we can make it so that agents are not pushing
messages into my chat or we are useless ... i cannot stand that text so help me
it floods my mind."

## Decision 2026-09-07

Chris: "i dont want boop wait i want boop to push things." The 2026-09-05 fix
swept a lane's end rows in with its progress rows, so a lane that died rc=1 left
`exited_without_completion` in the coordinator's mailbox with nothing pushing;
the coordinator found out by noticing the lane missing from `lane list`.

`MessageKind` now splits the supervisor kinds two ways
(`crates/boop-store/src/bus.rs`):

| classifier | kinds | ladder |
|---|---|---|
| `lane_end_row` | `result`, `completion`, `exited_without_completion`, `open_failed`, `retry_budget_exhausted` | the whole ladder, like a `hail`: door, door queue, turn boundary, hook inbox, mailbox, under the same door budget and cool-off |
| `lane_progress_row` | `yield`, `reparented`, `retrying`, `head_rewound` | Rung 0 `MailboxOnly`; read with `boop wait` |

Rung 0 in `land()` and the `drain_route_held_mail_budgeted` filter both read
`lane_progress_row()` now, so an end row retries through the drain and a
progress row still stamps one landing per row.

`## Expected` and `## Acceptance Criteria` keep their 2026-09-05 text and hold
for progress rows only: an end row now records `accepted-by-harness` at a live
door, and the drain retries it.

## Receipts

- `crates/boop-proc/src/deliver.rs`: `deliver_hail_budgeted` and `land` are the
  ladder. `land` reaches the `Door` rung for any route whose harness declares
  `MailPolicy::Door` and has a live session. `Rung` (:36-83) names the rungs and
  the transition each records. eb3c8b8 moved `to_timestamp` stamping into the
  ladder.
- `crates/boop-proc/src/supervise.rs`: `record_result` (:1294) and
  `mail_to_parent_kind` (:1529) are the supervisor's writers; both call
  `deliver_outbound` (:1620), which calls `deliver_hail`.
- `crates/boop-proc/src/deliver.rs`: `drain_route_held_mail_budgeted` re-walks
  the ladder for every unstamped row on every wrapper tick and every
  sync-carrying command, so even a row appended without delivery reaches the
  door on the next tick.
- 1270666 (merged from `feature/boop-quiet-yields`, 7948e82) quieted only
  `yield` and `head_rewound`, and only when the parent route's kind is
  `coordinator`. It set a `deliver` flag at the writer rather than at the
  ladder, so the drain still pushed the row, and `result` was never covered.
- `crates/boop-store/src/bus.rs`: `MessageKind` is an enum with `Other(String)`
  (0c126e7), so the switch can be typed rather than a string match.
- `crates/boop-proc/src/mailwait.rs`: `already_in_front_of_the_recipient` reads
  `held-in-mailbox` as still unread, which is what lets `boop wait --me` hand
  such a row back.
- `issues/head-rewound-door-retry/item.md` (closed 2026-09-05) for the ladder
  vocabulary and the same route's earlier retry loop.

## Expected

- Supervisor progress rows (`yield`, `reparented`, `retrying`,
  `head_rewound`) land in the mailbox with `to_timestamp` NULL and one
  `held-in-mailbox` transition, regardless of parent route kind.
- Lane end rows (`result`, `completion`, `exited_without_completion`,
  `open_failed`, `retry_budget_exhausted`) take the delivery ladder so the
  parent is pushed the outcome. Commit rows retain their delivery behavior.
- The classifier is `MessageKind`; no string matching on kind names.

## Acceptance Criteria

- [x] `MessageKind` carries the classifier; every supervisor kind is named there
      and nowhere else.
- [x] A progress row to a route with a live door records no
      `accepted-by-harness` transition; a request to the same route still takes
      the door.
- [x] A lane end row takes the door ladder and pushes the result to its parent.
- [x] `boop wait <lane>` returns the lane's rc from its result row; exit codes
      are unchanged.
- [x] The drain does not re-walk the ladder for a mailbox-only progress row.

## Reproduction on installed boop 0.0.10 (248dfdd3)

Read-only `~/.agent/boop.db` query: progress row `m-fb2cb469` (`yield`) has only
`appended -> held-in-mailbox`; lane end row `m-8f76ec53` (`result`) has
`appended -> held-for-turn-boundary`. This matches the 2026-09-07 ruling that
progress stays quiet while lane outcomes are pushed. Existing tests
`a_lane_progress_row_stays_off_the_door` and
`a_lane_end_row_takes_the_door_of_a_live_route` cover the split.
