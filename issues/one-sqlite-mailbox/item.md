---
created: 2026-08-24
updated: 2026-08-25
type: improvement
status: done
priority: high
epic: boop-one-path
labels: [domain-boop]
size: L
closed: 2026-08-25
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

### 2026-08-25T04:53:10Z · @feat-one-sqlite-mailbox

Follow-up commit e124229: the import tails the legacy files instead of claiming them.

An old `boop` keeps appending to `bus.ndjson` and `registry.json` for as long as one
runs, so the one-shot rename stranded every row written after it. Each open now reads
the ndjson from the last imported byte to the last complete line, and merges the
registry by name with the newer `registeredAt` winning. Nothing is renamed, moved or
deleted, and a row already in `agent_mail` is a no-op, so the two binaries run side by
side until nothing old is left.

Receipt on a copy of the live mailbox (3057 lines, 75 routes):

| open | rows | routes | ndjson offset |
|---|---|---|---|
| 1 | 2109 | 75 | 1405648 (whole file) |
| 2, after 2 rows appended | 2111 | 75 | 1406074 |
| 3, nothing appended | 2111 | 75 | 1406074 |

3059 ndjson lines fold to 2111 ids: the old format writes a second line per ack and the
upsert folds it onto the row. Each late row carries exactly one transition, zero rows
have none, and both files are still in place afterward.

`default_mail_dir` now follows `BOOP_MAIL_DIR`, then the directory `BOOP_DB` names, so a
redirected store redirects the mailbox with it and no verb reads `~/.agent` behind the
caller's back. `concatmap --me` takes `--mail-dir`.

Tests: 549 passed, 0 failed.

