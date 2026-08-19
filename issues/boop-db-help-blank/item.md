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

# Eight db subcommands have blank help

## Description

Eight `db` subcommands print no help text. `db turn list` and `db chat list` describe themselves with a struct doc comment that names two hidden verbs.

| field | value |
|---|---|
| audit row | section 9, row 15 |
| cost | S |
| needs Chris | no |

Sites:

- `crates/boop/src/main.rs:4113-4160`
- `crates/boop/src/main.rs:560`

## Acceptance Criteria

- [x] Every `db` subcommand has a one-line doc that says what it returns and against which table.
- [x] `db turn list` and `db chat list` carry their own help, naming no hidden verb.
- [x] A test walks the clap command tree and fails on any subcommand with an empty `about`.

## Tests Run

`cargo test -p boop --no-fail-fast` on fix/boop-main-fixes, exit 0, 420 passed
/ 0 failed / 1 ignored, 26.84s wall. `cargo clippy -p boop --all-targets` -> 1
warning, `tests/host_chat.rs:44` `needless_borrow`, present at daa2b0a.

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
