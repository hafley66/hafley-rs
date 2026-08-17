---
created: 2026-08-17
updated: 2026-08-17
type: bug
status: open
priority: normal
epic: boop-lane-observability
labels: [domain-boop, intent-correctness, needs-chris]
size: S
---

# one_shot is implemented by exactly one adapter

## Description

`one_shot` is implemented by exactly one harness adapter, so `concatmap --rules {"feed":"oneshot"}` silently only works on opencode. Every other harness takes the rule and does something else without saying so.

| field | value |
|---|---|
| audit row | section 9, row 29 |
| cost | S |
| needs Chris | yes |

Sites:

- `crates/boop/src/harness.rs:54` (trait method)
- `crates/boop/src/harness/opencode.rs:86` (the only impl)

## Fork

Is the single-adapter state intended? Chris's call. Do not dispatch.

## Acceptance Criteria

- [ ] Decided: either every adapter implements `one_shot`, or the trait method goes away and the rule errors on an unsupporting harness.
- [ ] No silent fallback: an unsupported `feed` rule fails loudly, test pinned.
- [ ] The `concatmap` help says which harnesses accept which feed rules.

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
