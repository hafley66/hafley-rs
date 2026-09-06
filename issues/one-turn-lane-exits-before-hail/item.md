---
created: 2026-09-05
updated: 2026-09-05
type: bug
status: open
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

One of, decided when the fix is written:

- The send refuses at once, naming the lane and saying the route is gone, so the
  coordinator respawns instead of waiting.
- Or a row to a retired lane revives it the way `revive_if_retired` already does
  for a retired route, and the lane resumes its conversation.
- Or the lane's route survives its exit in a `retired` state that a later hail
  can revive, and `lane list` shows it.

Whichever, the sender learns within one command that its hail cannot be
delivered, and the message names the next action.

## Acceptance Criteria

- [ ] A `boop beep <lane>` to a lane whose route is gone exits nonzero, or
      revives the lane; it does not print a landing and then sit.
- [ ] The printed line names the lane and the command that respawns or revives it.
- [ ] A test covers hail-after-exit for a one-turn lane whose `--expect-*` was
      met.
- [ ] The two receipt rows above are explained by the root cause the fix names.
