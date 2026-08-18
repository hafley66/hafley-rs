---
created: 2026-08-17
updated: 2026-08-17
type: bug
status: open
priority: normal
epic: boop-lane-observability
labels: [domain-boop, intent-correctness]
size: S
---

# Eight pure-read db call sites open the store read-write

## Description

Eight pure-read call sites covering 15 `db` verbs open the store read-write. `Store::open` re-runs the DDL batch and `PRAGMA user_version` on each call, so every one of them is a would-be writer under `journal_mode=delete`. Four other sites already use the read-only open.

| field | value |
|---|---|
| audit row | section 9, row 14 |
| cost | S |
| needs Chris | no |

Sites:

- `crates/boop/src/main.rs:5437`, `:5442`, `:5454`, `:5482`, `:5528`, `:5545`, `:5598`, `:5614`
- `crates/boop/src/main.rs:5977` (the read-only open that is the model)

## Fork

Related: `@boop-db-wal-lock`.

## Acceptance Criteria

- [ ] All eight move to `open_ro_store`.
- [ ] A test asserts a read verb succeeds against a store file opened read-only (or on a read-only filesystem path).
- [ ] No DDL batch runs on a read path.
- [ ] `cargo test -p boop -j4` green.

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
