---
created: 2026-08-17
updated: 2026-09-26
type: improvement
status: obsolete
priority: normal
epic: boop-lane-observability
labels: [domain-boop, intent-implementation]
size: M
closed: 2026-09-26
---

# channel/opencode.rs and channel/kimi.rs are impls that only carry data

## Description

`channel/opencode.rs` and `channel/kimi.rs` total 509 lines of `LaneChannel` impl that only carry a `TuiProfile`, which is already data.

| field | value |
|---|---|
| audit row | section 9, row 12 |
| cost | M |
| needs Chris | no |

Sites:

- `crates/boop/src/channel/opencode.rs:36`
- `crates/boop/src/channel/kimi.rs:36`
- `crates/boop/src/channel/tui.rs:504`, `:527`

## Acceptance Criteria

- [ ] Both files are deleted; the two profiles are `TuiProfile` values.
- [ ] Channel selection reads the profile table, no per-harness impl.
- [ ] The opencode and kimi lanes still spawn and receive mail; test pins the composed command for each.
- [ ] `cargo test -p boop -j4` green.

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.

## Resolution

### 2026-09-27T03:33:11Z · @issuectl

The referenced channel/opencode.rs, channel/kimi.rs and TuiProfile implementations do not exist in the current crate tree; ACP is the active channel path.
