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

# DOCTRINE hardcodes the schema version number

## Description

The `DOCTRINE` string constant hardcodes "this build writes version 10", duplicating `SCHEMA_VERSION`. A number written into a doc string is a number that rots.

| field | value |
|---|---|
| audit row | section 9, row 24 |
| cost | S |
| needs Chris | no |

Sites:

- `crates/boop/src/main.rs:200`
- `crates/boop/src/ident.rs:26` (`SCHEMA_VERSION`)

## Acceptance Criteria

- [x] The doctrine text interpolates `SCHEMA_VERSION` instead of naming a literal.
- [x] A test asserts `boop --help` output contains the current `SCHEMA_VERSION`.
- [x] No other literal schema version survives in `main.rs`; grep receipt in the PR body.

## Tests Run

`cargo test -p boop --no-fail-fast` on fix/boop-main-fixes, exit 0, 420 passed
/ 0 failed / 1 ignored, 26.84s wall. `cargo clippy -p boop --all-targets` -> 1
warning, `tests/host_chat.rs:44` `needless_borrow`, present at daa2b0a.

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
