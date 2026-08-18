---
created: 2026-08-17
updated: 2026-08-18
type: feature
status: open
priority: normal
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, needs-chris]
size: M
---

# concatmap is a coroutine: concatMap of run-pair bundles into an upserted resident session

## Description

Chris, 2026-08-18: the loop is one operator, `concatMap(bundle => resident.send(bundle))`.
The resident (small model) is a chat session; each reply is upserted as that session's next
turn in `~/.agent/boop.db`. Bundles are (ai run, user run) pairs: consecutive same-role turns
summed into one window. "Handled" is a query over the resident's turns. Nothing before the
model decides which bundles it sees.

Plan and target program: sprefa `plans/2026-08-18-boop-resident-coroutine.md`.

## What goes away in `crates/boop/src/concatmap.rs`

| today | site | target |
|---|---|---|
| cursor text file | `:621`, `:656-680` | gone; store bind cursor is the poll high-water |
| done set + `state/done/<session>-<turn>` markers | `:573`, `:585`, `:598`, `:615`, `:685-705` | gone; `handled(session, user_run) <- resident_reply(...)` |
| `coalesce_jobs(cap)` | `:566` | gone |
| `out/` files | `process_job` | gone; replies are turns |
| retry ladder / fixed-point | `REWRITE_ATTEMPTS`, `rewrite_*` rels in `boop-concatmap.dl6` | gone for the coroutine; `boop_oneshot` stays for one-shot programs |

## Acceptance Criteria

- [ ] `boop host chat --session <resident>` reads one JSON bundle on stdin, upserts it as the resident's next user turn through the harness channel, blocks until the reply turn lands, prints `(reply_turn, reply)` JSON.
- [ ] `resident-coroutine.dl6` (plan section 4) compiles rc=0 and its Rust golden runs against a store fixture with two runs per role.
- [ ] Order: two bundles in one tick reach the resident in `user_run` order (test pinned).
- [ ] `boop concatmap` cursor/done/coalesce code deleted; `boop concatmap` invokes the program.

## Fork, Chris

Order guarantee within a tick (host refuses out-of-order run vs one-demand-per-tick); store bind poll vs sqlite hook; base on `feature/dl6-boop-concatmap-golden` (sprefa `27b15b2`, hafley-rs `6b6315f`) or redo.
