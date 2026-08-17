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

# 34 --mail-dir declarations, three --format enums, two timeout spellings

## Description

`--mail-dir` is declared 34 times. `--format` is spelled by three separate enums. `--wait-timeout` and `--timeout` coexist with three different defaults.

| field | value |
|---|---|
| audit row | section 9, row 21 |
| cost | S |
| needs Chris | no |

Sites:

- `crates/boop/src/main.rs` (24 hits for the flag group)

## Acceptance Criteria

- [ ] `--mail-dir` is one global arg on `Cli`, declared once.
- [ ] One `--format` enum.
- [ ] One timeout spelling with one default, named as a constant.
- [ ] Old spellings keep working as clap aliases, or their removal is listed in the PR body.
- [ ] `cargo test -p boop -j4` green.

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
