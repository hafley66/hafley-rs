---
created: 2026-08-17
updated: 2026-09-05
type: improvement
status: done
priority: normal
epic: boop-lane-observability
labels: [domain-boop, intent-implementation]
size: S
closed: 2026-09-05
---

# Crate-level dead_code allows suppress the unused-trait-method signal

## Description

`#![allow(dead_code)]` sits at the top of `harness.rs` and `proc.rs`, suppressing exactly the unused-trait-method warning that would show which trait methods no adapter implements.

| field | value |
|---|---|
| audit row | section 9, row 23 |
| cost | S |
| needs Chris | no |

Sites:

- `crates/boop/src/harness.rs:2`
- `crates/boop/src/proc.rs:3`

## Acceptance Criteria

- [x] Both blanket allows removed.
- [x] Every resulting warning is either fixed by deleting the dead item or narrowed to a per-item `#[allow(dead_code)]` with a one-line reason.
- [x] The list of items that turned out to be dead is in the PR body.
- [x] `cargo test -p boop -j4` green and `cargo build -p boop` warning-free for `harness.rs`.

## Tests Run

Module-level `#![allow(dead_code)]` removed from boop-mux/src/lib.rs and harness/{codex,kimi,claude}.rs. Deleted dead items: `tmux_command` (boop-mux), `LivePeer`/`PeerKey` (claude.rs). Narrow `#[allow(dead_code)]` with reasons kept on: claude.rs `launch_command` (test-only), boop-mux test `Sink`/`sink`/`received` scaffolding (commit de70eb1). `boop-clippy -D warnings` clean on boop-mux and boop-harness.


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

`cargo clippy -p boop --all-targets -j4` -> 1 warning, in `tests/host_chat.rs` (`needless_borrow`), none in `harness.rs` or any harness adapter.

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
