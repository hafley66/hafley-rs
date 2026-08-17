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

# run_chat_query takes three bools and ignores one

## Description

`run_chat_query(&query, all, follow, json)` takes three positional bools and ignores `_json`.

| field | value |
|---|---|
| audit row | section 9, row 25 |
| cost | S |
| needs Chris | no |

Sites:

- `crates/boop/src/main.rs:1032`

## Acceptance Criteria

- [ ] The three bools become one options struct, or `--json` is honored and the parameter used.
- [ ] If `--json` was meant to work, it emits JSON and a test pins the shape; if it was never meant to exist, the flag is removed from the clap tree.
- [ ] No call site passes a bare `true, false, false`.
- [ ] `cargo test -p boop -j4` green.

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
