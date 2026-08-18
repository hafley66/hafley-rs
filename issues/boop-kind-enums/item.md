---
created: 2026-08-17
updated: 2026-08-17
type: improvement
status: open
priority: normal
epic: boop-lane-observability
labels: [domain-boop, intent-implementation]
size: S
---

# Message.kind and Route.kind are Strings matched against literals

## Description

`Message.kind` and `Route.kind` are `String` fields matched against string literals in four or more places. `lane_state` and `ParentPick.source` have the same shape.

| field | value |
|---|---|
| audit row | section 9, row 13 |
| cost | S |
| needs Chris | no |

Sites:

- `crates/boop/src/bus.rs:23`, `:32`
- `crates/boop/src/supervise.rs:80`
- `crates/boop/src/trail.rs:139`
- `crates/boop/src/main.rs:1989`

## Acceptance Criteria

- [ ] `Message.kind` and `Route.kind` are enums with serde rename to the existing wire strings.
- [ ] `lane_state` and `ParentPick.source` are enums too.
- [ ] Every literal match site becomes an exhaustive `match`; no `_ =>` arm that swallows an unknown kind silently.
- [ ] Unknown wire values from older on-disk rows still deserialize (explicit `Other(String)` or a documented hard error), pinned by a test.

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
