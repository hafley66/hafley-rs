---
created: 2026-08-17
updated: 2026-08-18
type: bug
status: done
priority: normal
epic: boop-lane-observability
labels: [domain-boop, intent-correctness]
size: S
closed: 2026-08-18
---

# concatmap splices a session id into SQL and swallows the error

## Description

`mapper_context_tokens` builds SQL by string interpolation of harness-controlled text, routes it through the human `passthrough` door, and swallows the error.

| field | value |
|---|---|
| audit row | section 9, row 5 |
| cost | S |
| needs Chris | no |

Sites:

- `crates/boop/src/concatmap.rs:258`

## Acceptance Criteria

- [x] The session id is a bound parameter, never interpolated.
- [x] The query runs against the typed store API, not the `passthrough` human door.
- [x] The error is returned or logged through `tracing`, never dropped.
- [x] Test with a session id containing a quote character; fail-first receipt in the test header.

## Tests Run

- [x] `cargo test -p boop --no-fail-fast`, lib binary: `test result: ok. 312 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.87s`
- [x] `concatmap::tests::context_tokens_handles_a_quote_in_the_session_id` ... ok

## Implementation Notes

Landed in PR #20, an ancestor of the base sha 69f00c1: `context_tokens` binds
`?1` through `store.connection().query_row` and logs its error with
`tracing::warn!`. Closing the card also cleared the last four `eprintln!` calls
out of `concatmap.rs`, which the standing law bans in `src/**`. The cursor file
and the `done/` marker directory are untouched; that is
`boop-concatmap-state-in-store`.

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
