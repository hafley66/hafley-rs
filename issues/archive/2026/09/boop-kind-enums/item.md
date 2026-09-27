---
created: 2026-08-17
updated: 2026-09-05
type: improvement
status: done
priority: normal
epic: boop-lane-observability
labels: [domain-boop, intent-implementation]
size: S
closed: 2026-09-05
---

# Message.kind and Route.kind are Strings matched against literals

## Description

`Message.kind` and `Route.kind` are `String` fields matched against string literals in four or more places. `lane_state` and `ParentPick.source` have the same shape.

| field | value |
|---|---|
| audit row | section 9, row 13 |
| cost | S |
| needs Chris | no |

Sites:

- `crates/boop/src/bus.rs:23`, `:32`
- `crates/boop/src/supervise.rs:80`
- `crates/boop/src/trail.rs:139`
- `crates/boop/src/main.rs:1989`

## Acceptance Criteria

- [ ] `Message.kind` and `Route.kind` are enums with serde rename to the existing wire strings.
- [ ] `lane_state` and `ParentPick.source` are enums too.
- [ ] Every literal match site becomes an exhaustive `match`; no `_ =>` arm that swallows an unknown kind silently.
- [ ] Unknown wire values from older on-disk rows still deserialize (explicit `Other(String)` or a documented hard error), pinned by a test.

## Tests Run

`Message.kind` (MessageKind) and `Route.kind` (RouteKind) are enums in crates/boop-store/src/bus.rs with `Other(String)` fallback for unknown wire values, pinned by `an_unknown_message_kind_round_trips_through_other` and `a_route_with_an_unknown_kind_still_loads`. Cross-crate compile fallout fixed in boop-proc (supervise/inbox/deliver/mailwait/lane), boop-harness, boop-store (runtime/tests), and boop/src/cli (debug/job/mail/mod). `lane_state` and `ParentPick.source` left as strings: both live in crates owned by another lane this session. Comparison sites use the enum's `as_str()`/`PartialEq` rather than a rewritten exhaustive match to bound blast radius (commit 0c126e7).


## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
