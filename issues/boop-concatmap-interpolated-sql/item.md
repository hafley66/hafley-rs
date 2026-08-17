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

# concatmap splices a session id into SQL and swallows the error

## Description

`mapper_context_tokens` builds SQL by string interpolation of harness-controlled text, routes it through the human `passthrough` door, and swallows the error.

| field | value |
|---|---|
| audit row | section 9, row 5 |
| cost | S |
| needs Chris | no |

Sites:

- `crates/boop/src/concatmap.rs:258`

## Acceptance Criteria

- [ ] The session id is a bound parameter, never interpolated.
- [ ] The query runs against the typed store API, not the `passthrough` human door.
- [ ] The error is returned or logged through `tracing`, never dropped.
- [ ] Test with a session id containing a quote character; fail-first receipt in the test header.

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
