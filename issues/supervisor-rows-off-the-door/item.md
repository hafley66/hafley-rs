---
created: 2026-09-05
updated: 2026-09-05
type: bug
status: open
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

- A supervisor row lands in the mailbox only: `agent_mail` row appended,
  `to_timestamp` NULL, exactly one landing transition saying where it waits.
- It never takes the door rung, the hook-inbox rung, or the pane-paste rung, for
  every parent route kind (`coordinator`, `native`, `lane`, `shell`).
- The parent reads it with `boop wait <lane>` (rc from the result row) or
  `boop wait --me` (the whole batch, one wake for the harness, no transcript
  text).
- A reply to a `boop beep`, and a human or agent hail, take the door exactly as
  today. The exemption is the message kind, never the route.
- The switch is `MessageKind`; no string matching on kind names.

## Acceptance Criteria

- [ ] `MessageKind` carries the classifier; every supervisor kind is named there
      and nowhere else.
- [ ] A `result` row to a route with a live door records no `accepted-by-harness`
      transition and opens no door.
- [ ] A `request` row to the same route on the same store still takes the door.
- [ ] `boop wait <lane>` returns the lane's rc from a result row whose only
      landing is `held-in-mailbox`; the exit codes are unchanged.
- [ ] `boop wait --me` returns yield, commit and result rows addressed to the
      caller.
- [ ] The drain does not re-walk the ladder for a supervisor row, so the ledger
      grows one landing per row, not one per tick.
- [ ] `cargo test -p boop-proc -p boop-store -p boop` at the repo's known-failure
      baseline.
