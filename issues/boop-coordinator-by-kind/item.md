---
created: 2026-08-17
updated: 2026-08-17
type: bug
status: open
priority: normal
epic: boop-lane-observability
labels: [domain-boop, intent-correctness]
size: S
---

# Coordinator is picked by name substring while Route.kind already says it

## Description

`resolve_parent` picks the coordinator by testing whether the lane name contains the word `coordinator`, while `Route.kind` already carries that fact.

| field | value |
|---|---|
| audit row | section 9, row 9 |
| cost | S |
| needs Chris | no |

Sites:

- `crates/boop/src/lane.rs:378`

## Acceptance Criteria

- [ ] `resolve_parent` selects on `Route.kind == coordinator` (typed, see `@boop-kind-enums`), never on a name substring.
- [ ] A lane named `coordinator-something` that is NOT kind=coordinator is not selected; test pins it.
- [ ] A kind=coordinator route named without the word IS selected; test pins it.
- [ ] `cargo test -p boop -j4` green.

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
