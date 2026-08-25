---
created: 2026-08-17
updated: 2026-08-25
type: bug
status: fixed
priority: normal
epic: boop-lane-observability
labels: [domain-boop, intent-correctness]
size: S
closed: 2026-08-25
closed_by: claude-5
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

- [x] `resolve_parent` selects on `Route.kind == coordinator` (typed, see `@boop-kind-enums`), never on a name substring.
- [x] A lane named `coordinator-something` that is NOT kind=coordinator is not selected; test pins it.
- [x] A kind=coordinator route named without the word IS selected; test pins it.
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

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
