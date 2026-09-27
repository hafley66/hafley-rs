---
created: 2026-08-17
updated: 2026-09-27
type: improvement
status: fixed
priority: normal
epic: boop-lane-observability
labels: [domain-boop, intent-implementation]
size: M
---

# One spawn described by four parallel structs with five names for the lane id

## Description

One spawn is described by four parallel structs (19, 22, 18 and 13 fields) with five names for the lane id, copied field by field.

| field | value |
|---|---|
| audit row | section 9, row 4 |
| cost | M |
| needs Chris | no |

Sites:

- `crates/boop/src/main.rs:1485` (`DispatchArgs`), `:2309` (`LaneArgs`)
- `crates/boop/src/harness.rs:184` (`SpawnSpec`)
- `crates/boop/src/bus.rs:30` (`Route`)

## Acceptance Criteria

- [ ] One spawn type, one name per field; the CLI arg structs derive from it rather than mirror it.
- [ ] The lane id has exactly one field name across the spawn path.
- [ ] No field-by-field copy function survives the change.
- [ ] `cargo test -p boop -j4` green.

## Tests Run

- `cargo check -p boop -j 2` passed.
- `cargo nextest run -p boop-store -j 2 -E 'test(/captured_legacy_registry_row_migrates_into_the_canonical_route_shape/)'` passed.
- `cargo nextest run -p boop-harness -j 2 -E 'test(/supervisor_command|worktree/)'` passed (18 tests).
- `cargo nextest run -p boop -j 2 -E 'test(/claude_fork_command_preserves|lane_create_dry_run_names/)'` passed (2 tests).

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.

## Decisions

### 2026-09-27T04:09:22Z · @codex

Repro: current Boop still defines separate `DispatchArgs`, `LaneArgs`, `boop_store::session::SpawnSpec`, and `bus::Route` types, with copied spawn fields at `run_dispatch`, `run_lane`, and route persistence. The user decided Route JSON adopts the canonical spawn type, both CLI paths and harness spawning use the same type, and old registry rows are migrated by reading captured old JSON and writing the new row shape.

Receipt: `DispatchArgs` and `LaneArgs` are aliases of the canonical `SpawnSpec`; `Route` aliases it too. Dispatch resolves the canonical value in place. The captured legacy registry row imports into SQLite and serializes back through the current camelCase Route JSON fields. Focused store, harness, and CLI tests passed.
