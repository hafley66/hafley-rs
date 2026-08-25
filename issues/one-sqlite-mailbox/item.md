---
created: 2026-08-24
updated: 2026-08-25
type: improvement
status: open
priority: high
epic: boop-one-path
labels: [domain-boop]
size: L
---

# One sqlite mailbox replaces bus.ndjson + registry.json

## Description

## Description

Rows live in `~/.agent/mail/bus.ndjson`, routes in `registry.json`, delivery receipts in sqlite `agent_delivery` / `agent_delivery_transition`. `boop debug` joins them and old rows print `no delivery transition`; the reconciler in `crates/boop-proc/src/deliver.rs` exists only to bridge the two stores.

Cut: one sqlite table per row with its transition list; append and first transition in one insert; `--mail-dir` becomes `--db`. Delete the reconciler.

## Acceptance Criteria

- [x] `bus.ndjson` and `registry.json` no longer written
- [x] a row cannot exist without a transition (schema constraint + test)
- [x] `boop debug <lane>` section 2 never prints `no delivery transition`
- [x] migration reads an existing ndjson once

## Agent Runs

### 2026-08-25T04:43:36Z · @feat-one-sqlite-mailbox

Branch feat/one-sqlite-mailbox, six commits on top of 9a2c03f.

| sha | step |
|---|---|
| 6838fef | schema 17: agent_mail, agent_route, agent_mail_needs_transition trigger |
| 0c3baa2 | routes on agent_route; one-shot ndjson/registry import on first open |
| 2fe28a3 | every send and ack goes through the store, append + first transition in one transaction |
| fd55118 | outbound reconciler deleted from supervise.rs |
| d9d9080 | debug section 2 is one inner join from agent_mail to its last transition |
| 6052699 | 12-process mailbox contention test |

Tests: 545 passed, 0 failed across boop, boop-proc, boop-store, boop-harness.

Contention: 12 OS processes x 50 appends + 25 acks against one db, wall 0.370s,
slowest single append 355ms, zero busy failures, zero rows without a transition.
Live: send latency 7-10ms with a lane supervisor polling; the 17-68s a ping waits
is the opencode turn boundary the row is held for, not the store.

Live receipt (scratch db, release binary d9d9080): lane side-flash4 wrote commit,
idle and result rows, each carrying a transition; a ping was appended, taken and
answered; boop debug printed all five sections with a landing on every mail row.

Note: --mail-dir now means the directory that holds boop.db. The default mail dir
maps to the one store beside it.
