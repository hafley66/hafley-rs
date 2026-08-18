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

# A general RFC-3339 parser lives inside the claude adapter

## Description

A general RFC-3339 parser lives inside the claude harness adapter and is imported from there by the codex adapter, ident and chat. `main.rs` carries a second copy.

| field | value |
|---|---|
| audit row | section 9, row 11 |
| cost | S |
| needs Chris | no |

Sites:

- `crates/boop/src/harness/claude.rs:352`
- `crates/boop/src/main.rs:2297` (`parse_iso_ms`, the second copy)

## Fork

Build-vs-buy: state in the PR body why a hand-rolled parser survives, or delete it for the crate.

## Acceptance Criteria

- [ ] One timestamp parser in a neutral module (or an established crate: `time`/`chrono` are already in the dependency tree, check before writing one).
- [ ] No harness adapter is imported by another adapter for a general helper.
- [ ] Both copies deleted.
- [ ] Table test over the formats seen in the live trail: `Z` suffix, offset form, fractional seconds, missing fraction.

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
