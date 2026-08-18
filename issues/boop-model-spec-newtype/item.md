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

# model@effort parsed in four places with two copies of the allowlist

## Description

The `model@effort` split happens in four places, two of which repeat the `low|medium|high` allowlist.

| field | value |
|---|---|
| audit row | section 9, row 16 |
| cost | S |
| needs Chris | no |

Sites:

- `crates/boop/src/lane.rs:269`, `:295`
- `crates/boop/src/channel/codex.rs:149` (`split_effort`)
- `crates/boop/src/harness/codex.rs:177`

## Acceptance Criteria

- [ ] One `ModelSpec` type with `FromStr`, holding model and an `Effort` enum.
- [ ] The four split sites call it; the allowlist exists once.
- [ ] A bad effort (`--model x@turbo`) fails at parse with the allowlist in the message, not silently downstream; test pins it.
- [ ] `cargo test -p boop -j4` green.

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
