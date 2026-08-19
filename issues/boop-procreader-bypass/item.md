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

# ProcReader bypassed by two call sites; descendent_count misspelled

## Description

Two call sites take a `&SysinfoSnapshot` directly and bypass the `ProcReader` trait, so the seam that would let process reads be faked in a test is not the only door. `descendent_count` is misspelled (English is `descendant`).

| field | value |
|---|---|
| audit row | section 9, row 27 |
| cost | S |
| needs Chris | no |

Sites:

- `crates/boop/src/main.rs:4812`, `:5114`
- `crates/boop/src/proc.rs:40`

## Acceptance Criteria

- [x] Both call sites go through `ProcReader`.
- [x] `descendent_count` renamed to `descendant_count` everywhere; grep receipt in the PR body.
- [x] A test uses a fake `ProcReader` to drive one of the two call sites, proving the seam is real.
- [x] `cargo test -p boop -j4` green.

## Tests Run

`cargo test -p boop --no-fail-fast` on fix/boop-main-fixes, exit 0, 420 passed
/ 0 failed / 1 ignored, 26.84s wall. `cargo clippy -p boop --all-targets` -> 1
warning, `tests/host_chat.rs:44` `needless_borrow`, present at daa2b0a.

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
