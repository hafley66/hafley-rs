---
created: 2026-08-17
updated: 2026-08-17
type: feature
status: open
priority: high
epic: boop-lane-observability
related: ['@lane-status-command', '@boop-pane-liveness']
labels: [domain-boop, component-cli, intent-implementation]
---

# boop debug: one-shot tail of recent WARN/ERROR plus --help banner

## Description

`boop --help` and every verb print nothing about what just went wrong. The
trail exists (`~/.agent/lanes/<lane>/supervise.log`, `agent_trace_event` rows
with `kind=error`), but a coordinator only learns of a flake, an aborted
stream, or a dead lane when it goes looking. Sample from the current trail
(grep ` WARN\| ERROR` over `~/.agent/lanes/*/supervise.log`, 122 lines):

| count | line shape |
|---|---|
| 13 | `opencode session unresolved at boot; a respawn will re-feed the brief` |
| 32 | `lane provider flake; resuming flake_resumes=N flake_resume_cap=5` |
| 18 | `opencode tui turn ended with an aborted stream` |

`agent_trace_event`: 36 rows of `kind=error` in the store today.

## Objective

A one-shot debug verb, plus a banner on `boop --help`, that surfaces the tail
of WARN/ERROR events and stat anomalies from the last N minutes (default 2)
so a coordinator sees "something is wrong" without opening a log.

## Shape (design fork, needs Chris)

- `boop debug [--since 2m] [--lane <name>]`: prints the tail of
  WARN/ERROR lines across `~/.agent/lanes/*/supervise.log` plus
  `agent_trace_event` `kind=error` rows, grouped by lane, newest last.
- `boop --help` (and optionally every verb's `--help`) prepends a one-line
  banner when the last 2 minutes contain any WARN/ERROR: `!! 3 lanes flaked
  in the last 2m: run boop debug`. Silent when clean.
- Stats candidates for the anomaly check: flake_resumes at cap, dup
  completion hails (`lane done` mailed twice, see sprefa coordinator inbox
  today), lanes DEAD=no-trail, `agent_trace_event` error rate.
- Reads only. Same store, same trail: no new sink. Canned report is named
  SQL under `boop db`, per the boop SQL law.

## Acceptance Criteria

- [ ] `boop debug` exists, `--since` and `--lane` filters, output grouped by lane.
- [ ] `boop --help` prints the warning banner when the window is non-empty and nothing when empty.
- [ ] Banner cost under 50ms on the current trail (`~/.agent/lanes` at 36 dirs).
- [ ] Test with a synthetic `supervise.log` containing WARN and ERROR lines inside and outside the window.
- [ ] Doc line in the `--help` DOCTRINE block.

## Tests Run

## Implementation Notes

Sources: `crates/boop/src/trail.rs` (SUPERVISE_LOG, lanes_root),
`agent_trace_event` schema in `~/.agent/boop.db`, `dict_trace_kind.value='error'`.
Related: `@lane-status-command` (absorbed lane-log-tail), `@boop-pane-liveness`.
