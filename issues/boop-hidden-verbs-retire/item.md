---
created: 2026-08-17
updated: 2026-09-26
type: improvement
status: obsolete
priority: normal
epic: boop-process
labels: [domain-boop, intent-implementation, needs-chris]
size: S
blocked_by: ['@boop-job-namespace']
closed: 2026-09-26
---

# 16 hidden pre-split verbs, three of them not aliases

## Description

16 hidden pre-split verbs survive; three are not aliases for anything (`sessions`, `tail`, `adopt`). `adopt` is the verb the `--help` doctrine block tells coordinators to run.

| field | value |
|---|---|
| audit row | section 9, row 10 |
| cost | M |
| needs Chris | yes |

Sites:

- `crates/boop/src/main.rs:325-611` (declarations)
- `crates/boop/src/main.rs:646-919` (dispatch)

## Fork

Retire versus promote is Chris's call. Do not dispatch.

## Acceptance Criteria

- [ ] Each of the 16 is either promoted to a `beep`/`db` home or deleted.
- [ ] `adopt`, `sessions` and `tail` have real homes before anything is removed.
- [ ] The DOCTRINE block names the surviving spelling.
- [ ] Deprecation path for the removed spellings decided and documented.

## Tests Run

## Implementation Notes

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.

## Resolution

### 2026-09-27T03:11:03Z · @issuectl

Repro receipt (boop 0.0.10 49124370-dirty): root help exposes `tui`, `beep`, `db`, `debug`, `me`, and `agent`; `boop adopt`, `boop sessions`, and `boop tail` each return unrecognized-subcommand, while the current doctrine documents their surviving homes.
