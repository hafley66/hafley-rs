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

# Shell quoting duplicated eight times across the spawn path

## Description

Shell quoting is duplicated eight times (nine with the double-quote variant); every spawn composes its command through one of the copies. `random_hex`, `now_ms`, `record` and `parse_iso_ms` are duplicated the same way.

| field | value |
|---|---|
| audit row | section 9, row 7 |
| cost | S |
| needs Chris | no |

Sites:

- `crates/boop/src/harness.rs:117`
- `crates/boop/src/lane.rs:262`
- `crates/boop/src/identity.rs:133`
- `crates/boop/src/main.rs:2559`
- `crates/boop/src/channel/tui.rs:540`
- `crates/boop/src/harness/claude.rs:139`
- `crates/boop/src/harness/codex.rs:197`
- `crates/boop/src/harness/opencode.rs:472`, `:478`

## Acceptance Criteria

- [ ] One `shell::quote` (and one double-quote variant if both are really needed); the eight copies are deleted.
- [ ] `random_hex`, `now_ms` and `record` each have one home.
- [ ] A table test over the quoting edge cases: single quote, space, newline, `$`, backtick, empty string.
- [ ] `cargo test -p boop -j4` green.

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
