---
created: 2026-09-14
updated: 2026-09-27
type: feature
status: done
priority: normal
closed: 2026-09-27
---

## Description

Committed durable expiring reminders use explicit existing routes, persist claims in the mailbox store, and require a turn-end or threaded reply before another occurrence can be claimed. No lane is created or revived by the runner. ACPX routes remain held because queue admission does not prove a recipient turn. The scheduler candidate review found no library that fits the existing single-mailbox transaction and turn-receipt contract.

## Scheduler candidates

| Candidate | Persistence | Fits one SQLite mailbox | Dependency weight | Maintenance |
|---|---|---|---|---|
| `tokio-cron-scheduler` | Optional PostgreSQL or NATS persistence; no SQLite backend | No; adding either persistence service duplicates the mailbox store | High; Tokio plus a separate persistence client/backend | Active crate, but persistence lifecycle is separate from Boop mailbox receipts |
| `apalis` + SQLite | Persistent SQLite task backend; cron stream is in-memory and can pipe to storage | Partial; SQLite is supported, but jobs and mailbox/turn receipts use different storage semantics and retry state | High; async worker, SQL backend, codec and worker stack | Active project with split core/backend/cron crates |
| `clokwerk` | In-memory scheduler; no persistence | No; restart loses scheduled state | Low; scheduler and time support | Small synchronous scheduler, no durable storage layer |
| Plain `due_at` rows in `boop.db`, drained by the existing supervisor tick | SQLite rows in the existing mailbox database | Yes; schedule claim, message append, expiry and delivery receipt can share the current store | None added | Local schema and tick code maintained with the mailbox implementation |

**Pick:** plain due rows. The libraries provide either ephemeral schedules or an independent persisted job queue; none fits the existing single-mailbox transaction and turn-receipt contract without adding a second scheduling state machine. `tokio-cron-scheduler` persistence backends are PostgreSQL/NATS; Apalis documents SQLite storage and an in-memory cron source; clokwerk documents an in-memory scheduler. Sources: [tokio-cron-scheduler persistence](https://docs.rs/crate/tokio-cron-scheduler/latest/source/postgres.md), [Apalis architecture](https://apalis.dev/docs/introduction/architecture), [clokwerk Scheduler](https://docs.rs/clokwerk/latest/clokwerk/struct.Scheduler.html).

## Acceptance Criteria

- [x] `boop beep remind add/list/cancel/run` manages expiring schedules and persists a single outstanding envelope per route.
- [x] Runner restart does not repeat uncertain delivery, and a known refusal retries the same envelope.
- [x] Cancelled or expired reminders are hidden from held-mail reads and blocked at delivery and lane turn boundaries.
- [x] Missing/dead lanes and ACPX routes do not spawn agents or append reminder mail.
- [x] CLI help documents the reminder contract.
- [x] Schema version 36 upgrades to schema version 37 with the reminder table available.
- [x] Candidate table records persistence, mailbox fit, dependency weight and maintenance; plain due rows selected because the libraries require ephemeral schedules or separate scheduling state.

## Tests Run

- [x] `cargo nextest run -p boop-store -j 2 -E 'test(/reminder/)'` (9 passed)
- [x] `cargo nextest run -p boop-proc -j 2 -E 'test(/cancelled_reminder|reminder_buffer|reminder_turn_end/)'` (3 passed)
- [x] `cargo nextest run -p boop -j 2 -E 'test(/t1_reminder::/)'` (3 passed)
- [x] Candidate review committed as `b52dcee8` before closing this item.

## Implementation Notes

Schema version 37 stores schedules. The runner keeps one lock per mail directory, caps schedules and completion observers at 32, and uses existing delivery receipts. Lane supervisors write turn-end receipts into their own mail store. Focused verification used the changed Boop packages only.
