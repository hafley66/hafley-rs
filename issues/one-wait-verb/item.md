---
created: 2026-08-24
updated: 2026-08-25
type: improvement
status: open
priority: high
epic: boop-one-path
labels: [domain-boop]
size: S
---

# One wait: boop wait <id|lane>, and beep blocks

## Description

## Description

`wait <id>`, `wait --me`, `beep lane wait <lane>`, `lane create --wait`, `beep --timeout`. Help drifted (`--wait-timeout` vs `--timeout`, fixed 05f72c0).

Cut: `boop wait <id|lane|--me> [--timeout]` is the only blocking verb besides `beep`; `lane wait` and `create --wait` hidden aliases.

## Acceptance Criteria

- [x] `wait` accepts a lane id and exits with its rc
- [x] clap help-example test covers every wait spelling

## Agent Runs

### 2026-08-25T04:18:37Z · @chore-verb-cuts

7a832ea boop wait <id> dispatches to run_lane_wait when <id> names a registered lane route; beep lane wait and lane create --wait folded to hidden aliases; doctrine and clap help-example test cover boop wait <lane>.
