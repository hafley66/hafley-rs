---
created: 2026-08-24
updated: 2026-08-24
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

- [ ] `wait` accepts a lane id and exits with its rc
- [ ] clap help-example test covers every wait spelling
