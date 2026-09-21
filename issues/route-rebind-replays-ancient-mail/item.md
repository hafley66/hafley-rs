# Rebinding a route name replays its entire unacked backlog as fresh mail

## What happened

A coordinator session was cleared and the route `sprefa-coordinator` was rebound
to the new session with `boop beep lane patch`. The route had carried that name
since August. Every unacked row addressed to it then drained into the new
session at once, presented as live traffic.

At the moment of rebind:

```
select count(*), min(from_timestamp), max(from_timestamp), count(distinct from_route)
  from agent_mail where to_route='sprefa-coordinator' and to_timestamp is null
-- 350 rows, 2026-08-24T14:15:53Z .. 2026-09-21T13:48:06Z, 206 routes
```

The delivered rows carry no age marker. The receiving agent sees
`[boop m-xxxxxxxx from <lane>] <body>` with no timestamp, so a message written
on 2026-08-23 is indistinguishable from one written a minute ago.

## Why it bites

The coordinator read four month-old `BLOCKED` rows as pending decisions and
escalated them to the user as live blockers. All four had in fact resolved
themselves within the hour, four weeks earlier: the same routes later sent
`done rc=0` and shipped PRs #433, #434, #435. Those resolutions were also in the
backlog, arriving after the blockers, so the escalation was wrong at the moment
it was made and the correction arrived later in the same flood.

Cost is not only the wrong escalation. The flood interleaves with real work and
crowds the receiving context; roughly thirty stale rows landed inside a single
turn.

## Root cause, as far as this report can see

`to_timestamp is null` is the only liveness predicate on delivery. Nothing
bounds a row's age at drain time, and nothing marks a delivered row as stale to
the reader. A route name is treated as a durable address whose mailbox survives
the death of every session that ever held it, which is the right model for a
short gap and the wrong one for a four-week gap.

## Fix shape, for the owner to judge

Any one of these stops the failure; the first two are independent of each other.

1. Stamp the age into the delivered envelope. `[boop m-xxxx from <lane>, sent
   2026-08-23T19:10Z, 29 days ago]`. Cheap, and it makes every other case
   self-diagnosing.
2. Bound the drain by age on rebind. A row older than the new binding's
   creation, or older than some window, is delivered as a digest rather than as
   individual live rows, or is expired outright.
3. Do not inherit a mailbox across a rebind. `lane patch` onto a dead route
   could expire the backlog by default and say how many rows it dropped.

## Workaround in use

```
boop beep message ack --box <route> --max-age-days 2
```

This took the same box from 350 unacked rows to 44. It is bulk-mark, so it
proves no read; that is acceptable here precisely because the rows are dead.

## Rail

A test that binds a route, writes mail older than the window, rebinds the route
to a new session, and asserts the new session receives a digest or nothing
rather than N live rows.
