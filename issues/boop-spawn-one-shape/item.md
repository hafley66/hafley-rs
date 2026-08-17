---
created: 2026-08-17
updated: 2026-08-17
type: improvement
status: open
priority: normal
epic: boop-lane-observability
labels: [domain-boop, intent-implementation]
size: M
---

# One spawn described by four parallel structs with five names for the lane id

## Description

One spawn is described by four parallel structs (19, 22, 18 and 13 fields) with five names for the lane id, copied field by field.

| field | value |
|---|---|
| audit row | section 9, row 4 |
| cost | M |
| needs Chris | no |

Sites:

- `crates/boop/src/main.rs:1485` (`DispatchArgs`), `:2309` (`LaneArgs`)
- `crates/boop/src/harness.rs:184` (`SpawnSpec`)
- `crates/boop/src/bus.rs:30` (`Route`)

## Acceptance Criteria

- [ ] One spawn type, one name per field; the CLI arg structs derive from it rather than mirror it.
- [ ] The lane id has exactly one field name across the spawn path.
- [ ] No field-by-field copy function survives the change.
- [ ] `cargo test -p boop -j4` green.

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
