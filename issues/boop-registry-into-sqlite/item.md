---
created: 2026-08-17
updated: 2026-09-05
type: improvement
status: done
priority: normal
epic: boop-lane-observability
labels: [domain-boop, intent-implementation, needs-chris]
size: L
closed: 2026-09-05
---

# Lane registry is a hand-rolled JSON CAS beside a SQLite store

## Description

The lane registry is a hand-rolled JSON content-addressed store with dual key spellings, a `DefaultHasher` named `sha256_hex`, and hand-mirrored read/write shapes, sitting beside a SQLite database.

| field | value |
|---|---|
| audit row | section 9, row 3 |
| cost | L |
| needs Chris | yes |

Sites:

- `crates/boop/src/bus.rs:57`, `:72`, `:251`, `:290`
- `crates/boop/src/main.rs:2677`

## Fork

Blocked on `@boop-db-wal-lock`. The dual key spellings here are the suspected cause of `@boop-dup-completion-hail`.

## Acceptance Criteria

- [ ] Routes live in a table in `~/.agent/boop.db` with an INTEGER surrogate key; the natural lane name is UNIQUE in one dictionary table.
- [ ] `sha256_hex` is either a real digest or renamed to what it is.
- [ ] One key spelling for a lane, decided and enforced at the write seam.
- [ ] Migration reads the existing `registry.json` once and does not lose a live route.

## Tests Run

`bus::write_route` now upserts `agent_route` directly via `upsert_route`; `import_legacy`/`import_registry_file` retained for old mail dirs. `sha256_hex` renamed `hash_hex` (it is a DefaultHasher, not SHA-256). `cas_update_json` retained for `lane-residency.json`/`parent-policy.json`. Module doc fixed. deliver.rs/control.rs/me.rs callers compile unchanged (commit 6dacddd).


## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
