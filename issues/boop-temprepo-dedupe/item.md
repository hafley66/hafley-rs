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

# TempRepo test fixture triplicated across three adapters

## Description

The `TempRepo` test fixture is copied into three harness adapter test modules while `test_support.rs` already exists.

| field | value |
|---|---|
| audit row | section 9, row 18 |
| cost | S |
| needs Chris | no |

Sites:

- `crates/boop/src/harness/claude.rs:517`
- `crates/boop/src/harness/codex.rs:726`
- `crates/boop/src/harness/opencode.rs:864`
- `crates/boop/src/test_support.rs`

## Acceptance Criteria

- [x] One `TempRepo` in `test_support.rs`; the three copies are deleted.
- [x] Test count before and after is identical, quoted in the PR body.
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

`cargo test -p boop -j4 --lib -- codex` -> 21 passed, 0 failed.

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
