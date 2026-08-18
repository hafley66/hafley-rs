---
created: 2026-08-17
updated: 2026-08-17
type: improvement
status: open
priority: normal
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, needs-chris]
size: M
---

# concatmap loop memory is a text file and a directory of empty markers

## Description

concatmap loop memory (the cursor and the done set) lives as a text file plus a directory of empty marker files. A permanently-failed pair writes the SAME marker as a successfully mapped one, so the loop cannot tell them apart.

| field | value |
|---|---|
| audit row | section 9, row 17 |
| cost | M |
| needs Chris | yes |

Sites:

- `crates/boop/src/concatmap.rs:649`, `:678`, `:686`, `:745`, `:751`

## Fork

Overlaps `@boop-hosted-in-dl6`. Store choice needs Chris. Do not dispatch.

## Acceptance Criteria

- [ ] Cursor and done set are store relations with an INTEGER surrogate key.
- [ ] A failed pair records a distinct outcome from a mapped pair.
- [ ] Retry policy for a failed pair is decided and named.
- [ ] Existing on-disk markers migrate or are explicitly abandoned with a stated reason.

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
