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

- [x] The three bools become one options struct, or `--json` is honored and the parameter used.
- [x] If `--json` was meant to work, it emits JSON and a test pins the shape; if it was never meant to exist, the flag is removed from the clap tree.
- [x] No call site passes a bare `true, false, false`.
- [x] `cargo test -p boop -j4` green.

## Tests Run

`cargo test -p boop --no-fail-fast` on fix/boop-main-fixes, exit 0, 420 passed
/ 0 failed / 1 ignored, 26.84s wall. `cargo clippy -p boop --all-targets` -> 1
warning, `tests/host_chat.rs:44` `needless_borrow`, present at daa2b0a.

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
