---
created: 2026-08-19
updated: 2026-08-19
type: feature
status: open
priority: high
epic: boop-process
size: M
blocked_by: ['@boop-crate-split']
---

# boop job / boop mail / boop me: the job-control verb surface

## Description

## Description
The verb surface of `docs/design/boop-process.md` section 2: `boop job create|list|get|wait [<job>...]|kill|signal|rm|attach|pane`, `boop mail send|recv|wait`, `boop me whoami|mood|favorite|register`; `db`, `debug`, `config`, `host chat` unchanged; old spellings become hidden aliases for one release. New mechanisms: `wait` with no args = all my live children; `kill` keeps the row, `rm` forgets (carcass-safe, #35); `signal <sig> --children` fans out through parent edges; `attach` = tmux attach to the job's pane; `create --timeout <s>` per job overriding the 300s stall constant.
## Acceptance Criteria
- [ ] every row of the section-2 table has its verb; `boop --help` lists exactly `job mail me db debug config host help`.
- [ ] each old spelling is a hidden alias that prints one deprecation line to stderr and works.
- [ ] tests: wait-all with two children (one fails, rc propagates), kill keeps the row / rm forgets, signal --children reaches two live children and skips a dead one, attach on a pane-less job is a named error, --timeout kills at N+poll.
- [ ] `docs/design/boop-process.md` section 2 updated to match; `crates/boop/docs/*.md` verbs renamed.
