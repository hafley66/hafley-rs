---
created: 2026-09-05
updated: 2026-09-05
type: bug
status: closed
priority: high
epic: boop-process
closed: 2026-09-05
closed_by: opus-door-retry
---

# A `head_rewound` mail row retries the claude door every 5 s while the parent's turn is live

## Description

Lane `refactor-harness-transcript` ran `git commit --amend` (f3482b6 -> a3165f7).
HeadWatch wrote one mail row `m-0981e18a`, kind `head_rewound`, body
`head refactor-harness-transcript rewound: a3165f7 does not descend from the last reported f3482b6`.

The row has `to_route` = `claude-498` and `to_timestamp` NULL. The parent's door queue
(claude-498, `boop tui` wrapper in pane %498) records
`held-for-turn-boundary / door queue` every 5 s: 101 transitions between
12:13:14 and 12:21:38 local, still running. Each attempt also surfaces the body
in the parent's claude transcript as a peer message, so the parent sees the
same line 100+ times and its turn never reaches a boundary.

`boop wait --me` for claude-498 times out with the row present (it does not
take `yield`, `result`, or `head_rewound` kinds), and `boop wait <lane>` prints
the row without stamping `to_timestamp`, so nothing ever takes it. The same
holds for every supervisor row: after lane B, 6 untaken rows to claude-498
(`m-e7c7283a m-0981e18a m-00ce1184 m-4b93c9bc m-a94f62ac m-dbe222e9`), each
re-pushed through the door every 5 s (193 transitions on the oldest). `sqlite3 UPDATE agent_mail SET to_timestamp` was
blocked by the parent's permission classifier.

## Expected

- A supervisor row that reached the parent's transcript through the door is
  stamped taken (`to_timestamp`) on that push.
- The door gate from b3a6d22 trips once for a held row and reports the cool-off;
  a row that already reached the transcript must not re-push on every tick.

## Receipts

```
sqlite3 ~/.agent/boop.db "SELECT to_route, to_timestamp, kind FROM agent_mail WHERE message_id='m-0981e18a'"
claude-498||head_rewound
sqlite3 ~/.agent/boop.db "SELECT count(*) FROM agent_delivery_transition WHERE message_id='m-0981e18a'"
101
```

Killing `boop wait` processes (pids 89713, 96426) did not stop the retries; the
lane B supervisor (93975) and the parent's own `boop tui` wrapper remained.

## Root cause (2026-09-05, opus-door-retry)

Two separate faults, one per symptom.

**The 5 s retry was a stale process, not the code at ca2a7d8.** Every one of
the 1073 retry transitions reads `held-for-turn-boundary` / `door queue`. No
code since afc89ee (2026-09-03 15:42) can write that pair: `Rung::DoorQueue`
records `accepted-by-harness`. The same store holds 3 rows written the same
morning by a rebuilt binary (`accepted-by-harness` / `door queue`, 12:03:24 to
12:03:26), so two binaries were writing at once and only the old image looped.
It also predates the door budget (3f76d4d, 2026-09-03 23:22), which answers
why the gate never tripped inside that process: the image had no gate. A
gate-carrying binary that touched the same route did trip, on the pre-fix rows'
own count: `agent_door_blowout` id 3, `13 door pushes in 60s against 2 live
connects`, 12:28:36. Retries stopped when the wrapper was restarted at
18:20:53. `bus::held_messages` (b3a6d22) plus the gate hold the rail in code;
`a_row_a_door_already_queued_is_never_pushed_again` passes at ca2a7d8 against
the exact six-row shape, and `the_gate_counts_a_pre_fix_door_queue_row` now
pins that the gate reads pre-fix rows as pushes.

**The unstamped rows were a real hole in the ladder.** `to_timestamp` was
stamped by two callers of `deliver_hail` (the drain, and `boop beep` in
`cli/mail.rs`) and by neither of the others. The supervisor's parent hail
(`supervise.rs::deliver_outbound`, the path that writes `yield`, `result` and
`head_rewound`) called the ladder and stamped nothing, so `m-e7c7283a` sat
open with `accepted-by-harness` in its ledger. Such a row is owned by no
reader: `bus::held_messages` drops it because its ledger names a door, and
`boop wait --me` drops it because `already_in_front_of_the_recipient` reads
`landed()`. The stamp moved into `deliver_hail_budgeted` itself, so every
caller of the ladder stamps alike, and schema v26 backfills the rows written
before the move (the six live rows included).

`NOT_UNREAD_KINDS` is not the reason `boop wait --me` timed out: it is
`["ack", "dispatch"]`, so `yield`, `result` and `head_rewound` are already
unread by kind. The filter that dropped them is the ledger read at
`crates/boop/src/cli/job.rs:614` (defect 3, addendum 2026-08-25), and that is
the design: a row a door took is read in the transcript, not at a wait. A
coordinator whose door is gone lands at `turn boundary` instead, keeps its
`held_messages` place, and leaves through the next drain. No change made
there.

