---
created: 2026-09-14
updated: 2026-09-27
type: feature
status: done
priority: normal
closed: 2026-09-27
---

## Description

Ported the durable expiring reminder slice from the retained historical branch. Schedules use explicit existing routes, persist claims in the mailbox store, and require a turn-end or threaded reply before another occurrence can be claimed. No lane is created or revived by the runner. ACPX routes remain held because queue admission does not prove a recipient turn.

## Acceptance Criteria

- [x] `boop beep remind add/list/cancel/run` manages expiring schedules and persists a single outstanding envelope per route.
- [x] Runner restart does not repeat uncertain delivery, and a known refusal retries the same envelope.
- [x] Cancelled or expired reminders are hidden from held-mail reads and blocked at delivery and lane turn boundaries.
- [x] Missing/dead lanes and ACPX routes do not spawn agents or append reminder mail.
- [x] CLI help documents the reminder contract.
- [x] Schema version 36 upgrades to schema version 37 with the reminder table available.

## Tests Run

- [x] `cargo nextest run -p boop-store -j 2 -E 'test(/reminder/)'` (9 passed)
- [x] `cargo nextest run -p boop-proc -j 2 -E 'test(/cancelled_reminder|reminder_buffer|reminder_turn_end/)'` (3 passed)
- [x] `cargo nextest run -p boop -j 2 -E 'test(/t1_reminder::/)'` (3 passed)

## Implementation Notes

Schema version 37 stores schedules. The runner keeps one lock per mail directory, caps schedules and completion observers at 32, and uses existing delivery receipts. Lane supervisors write turn-end receipts into their own mail store. Focused verification used the changed Boop packages only.
