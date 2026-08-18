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

- [ ] One `TempRepo` in `test_support.rs`; the three copies are deleted.
- [ ] Test count before and after is identical, quoted in the PR body.
- [ ] `cargo test -p boop -j4` green.

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
