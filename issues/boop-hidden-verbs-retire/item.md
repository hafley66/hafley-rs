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

# 16 hidden pre-split verbs, three of them not aliases

## Description

16 hidden pre-split verbs survive; three are not aliases for anything (`sessions`, `tail`, `adopt`). `adopt` is the verb the `--help` doctrine block tells coordinators to run.

| field | value |
|---|---|
| audit row | section 9, row 10 |
| cost | M |
| needs Chris | yes |

Sites:

- `crates/boop/src/main.rs:325-611` (declarations)
- `crates/boop/src/main.rs:646-919` (dispatch)

## Fork

Retire versus promote is Chris's call. Do not dispatch.

## Acceptance Criteria

- [ ] Each of the 16 is either promoted to a `beep`/`db` home or deleted.
- [ ] `adopt`, `sessions` and `tail` have real homes before anything is removed.
- [ ] The DOCTRINE block names the surviving spelling.
- [ ] Deprecation path for the removed spellings decided and documented.

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
