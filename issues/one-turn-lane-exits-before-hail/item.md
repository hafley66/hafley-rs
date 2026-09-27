---
created: 2026-09-05
updated: 2026-09-05
type: bug
status: fixed
priority: normal
epic: boop-process
---

# A one-turn lane exits and deletes its route, so a later hail is held forever

## Description

## Description

A lane spawned with `--expect-*` whose expectations are met at the end of its
first turn exits there and deletes its registry route. A coordinator that reads
the lane's `idle` row and hails back in the next breath addresses a route that no
longer exists: the send lands `held-in-mailbox (no registry route)` and stays
there forever, because nothing ever re-creates that route and no drain can push
a row to a name the registry does not carry.

The coordinator has no way to tell this apart from a lane that is merely slow:
both look like a send that landed and an answer that never came.

## Receipts

- `m-da1a54ff` to `feature-turn-cwd`, 2026-09-05: appended, landed
  `held-in-mailbox`, detail `no registry route for feature-turn-cwd`.
- `m-86333624` to `feature-fork-render`, 2026-09-05: same shape.
- `crates/boop-proc/src/deliver.rs`, `land`: a `to` with no entry in `routes`
  returns `Landing::new(Rung::Mailbox, "no registry route for {to}")`. That is
  the terminal rung; the drain skips the route because it has no row to read.
- `crates/boop-proc/src/supervise.rs`, the one-turn exit path: the lane writes
  its `result` row, then the route is removed.

## Expected

The one-turn exit drops the live route and records retired residency plus a
spawn receipt. `boop beep <lane>` re-registers and revives that lane on its
pinned conversation before delivering the body, then returns on the lane's
result row. `lane list` shows the retired lane and its revive spelling.

## Acceptance Criteria

- [x] A `boop beep <lane>` to a lane whose route is gone revives it; it does not
      print a landing and then sit.
- [x] The printed line names the lane and says it is reviving the lane.
- [x] A test covers hail-after-exit for a one-turn lane whose `--expect-*` was
      met.
- [x] The two receipt rows above share the retired-route cause and revive from
      the stored spawn receipt.

## Reproduction on installed boop 0.0.10 (248dfdd3)

`boop beep lane get feature-turn-cwd` and `feature-fork-render` show both
receipts as `retired` with no session id. The fixture test
`a_finished_lane_retires_and_a_beep_revives_it_on_the_same_conversation` passed:
the one-turn lane drops its route, the next beep revives it on the pinned ACP
conversation, delivers the body, and returns the result row.
