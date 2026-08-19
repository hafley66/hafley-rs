---
created: 2026-08-17
updated: 2026-08-18
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

- [x] One `ModelSpec` type with `FromStr`, holding model and an `Effort` enum.
- [x] The four split sites call it; the allowlist exists once.
- [x] A bad effort (`--model x@turbo`) fails at parse with the allowlist in the message, not silently downstream; test pins it.
- [x] `cargo test -p boop -j4` green.

## Tests Run

`cargo test -p boop -j4 --no-fail-fast` at 69f00c1, exit 0, 402 passed / 0
failed / 1 ignored, zero build warnings:

| target | result |
|---|---|
| lib | 311 passed, 0 failed |
| bin | 47 passed, 0 failed |
| 0_sqlite_contention | 1 passed |
| bench_grid | 2 passed |
| concatmap_e2e | 0 passed, 1 ignored |
| coordinator_ping | 3 passed |
| host_chat | 3 passed |
| inbox_hooks | 8 passed |
| install_rail | 8 passed |
| lane_completion_row | 1 passed |
| lane_wait_exit | 7 passed |
| native_agent_liveness | 1 passed |
| registry_kinds | 3 passed |
| wait_mail | 7 passed |
| doc-tests | 0 passed |

`cargo test -p boop -j4 --lib -- lane::tests` -> 22 passed, 0 failed.
`cargo test -p boop -j4 --lib -- codex` -> 21 passed, 0 failed.

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
