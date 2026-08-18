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

- [ ] Both blanket allows removed.
- [ ] Every resulting warning is either fixed by deleting the dead item or narrowed to a per-item `#[allow(dead_code)]` with a one-line reason.
- [ ] The list of items that turned out to be dead is in the PR body.
- [ ] `cargo test -p boop -j4` green and `cargo build -p boop` warning-free for these two files.

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
