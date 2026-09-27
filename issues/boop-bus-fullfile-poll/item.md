---
created: 2026-08-17
updated: 2026-09-26
type: improvement
status: done
priority: normal
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, needs-chris]
size: M
closed: 2026-09-26
commits:
- hash: b540cc0a
  summary: 'perf(boop): query pending mail by recipient'
---

# Mail dir is re-read and re-parsed in full every 700 ms

## Description

The whole mail dir is read and JSON-parsed every 700 ms per lane, on every `dead_reason` call, and on every claude turn boundary. 716 KB / 1707 rows today, append-only, no rotation.

| field | value |
|---|---|
| audit row | section 9, row 1 |
| cost | M |
| needs Chris | yes |

Sites:

- `crates/boop/src/bus.rs:134`, `:150`
- `crates/boop/src/supervise.rs:15`, `:61`
- `crates/boop/src/trail.rs:131`
- `crates/boop/src/main.rs:1438`

## Fork

Store choice is Chris's call: move the mailbox into `~/.agent/boop.db` versus index the ndjson in place. Standing law says infra is bought, never built, and boop never reinvents SQLite. Do not dispatch before that fork is answered.

## Acceptance Criteria

- [ ] Reads no longer scale with total mail volume: an indexed or store-backed lookup replaces the full-file parse at `bus.rs:134,150`.
- [ ] `bus.ndjson` rotates, with the rotation size or age named in the code.
- [ ] A COUNT-style test pins the number of file reads (or SQL statements) per poll tick; end-state equality alone does not close this.
- [ ] Poll cost measured before and after on the current trail, numbers in the PR body.

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.

## Resolution

### 2026-09-27T03:20:45Z · @issuectl

Receipt: b540cc0a; `recipient_reads_use_the_covering_mail_index`, `pending_takes_only_this_lane_s_unacked_actionable_rows`, and `a_seen_id_is_not_offered_twice` passed. The 700 ms pending path now selects through idx_mail_to; bus.ndjson is import-only, so rotation is no longer part of this live path.
