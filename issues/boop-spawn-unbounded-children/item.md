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

# Three unbounded child processes in the spawn path

## Description

Three children in the spawn path run with no deadline: `just --show`, `just boop-start`, and `sh -c <setup>`. The 10-second law says any single operation over 10s is a defect to investigate, never a budget.

| field | value |
|---|---|
| audit row | section 9, row 8 |
| cost | S |
| needs Chris | no |

Sites:

- `crates/boop/src/worktree.rs:62`, `:83`, `:119`

## Acceptance Criteria

- [ ] Each of the three spawns carries a deadline; on expiry the child is killed and the lane fails with a named error.
- [ ] The timeout value is one named constant, not three literals.
- [ ] Test that a `sh -c 'sleep 999'` setup command fails the spawn within the deadline instead of hanging.
- [ ] Nothing seizes the machine: the killed child leaves no orphan (verify the process group is signalled).

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
