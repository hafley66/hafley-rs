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

# Lane exit code round-trips as prose and is recovered by three parsers

## Description

The lane exit code is written into a message body as prose (`lane <name> done rc=0`) and recovered by three separate string parsers.

| field | value |
|---|---|
| audit row | section 9, row 2 |
| cost | S |
| needs Chris | no |

Sites:

- `crates/boop/src/supervise.rs:337` (writer)
- `crates/boop/src/main.rs:5058` (parser)
- `crates/boop/src/trail.rs:118` (parser)

## Acceptance Criteria

- [x] `bus::Message` carries typed `rc: Option<i32>` and `detail` fields.
- [x] The three prose parsers are deleted; no call site re-derives rc from a message body.
- [x] Existing on-disk messages without the typed fields still read (serde default), pinned by a test over a fixture line captured from the live `bus.ndjson`.
- [x] `cargo test -p boop -j4` green.

## Tests Run

`cargo test -p boop --no-fail-fast` on fix/boop-main-fixes, exit 0, 420 passed
/ 0 failed / 1 ignored, 26.84s wall. `cargo clippy -p boop --all-targets` -> 1
warning, `tests/host_chat.rs:44` `needless_borrow`, present at daa2b0a.

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
