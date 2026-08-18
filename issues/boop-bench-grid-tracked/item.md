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

# crates/boop/target/bench-grid.md is tracked and rewritten by cargo test

## Description

`crates/boop/target/bench-grid.md` is TRACKED: `.gitignore` only ignores `/target` at the repo root, so a nested `crates/boop/target` is not covered. Any `cargo test -p boop` rewrites it and dirties the tree. The dispatch doctrine tells coordinators to prove lane work with `git status`, so this destroys the check.

| field | value |
|---|---|
| audit row | section 9, row 28 |
| cost | S |
| needs Chris | no |

Sites:

- `.gitignore:1`
- `crates/boop/target/bench-grid.md`

## Acceptance Criteria

- [ ] `.gitignore` ignores `target/` at any depth.
- [ ] `crates/boop/target/bench-grid.md` is `git rm --cached`-ed out of the index.
- [ ] `cargo test -p boop -j4` followed by `git status --porcelain` prints nothing.
- [ ] If the bench grid is wanted as an artifact, it writes somewhere outside `target/` and that path is named.

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
