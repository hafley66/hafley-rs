---
created: 2026-09-20
updated: 2026-09-20
type: bug
status: open
priority: normal
labels: [boop]
---

# boop: lane delete --dry-run deletes the lane

## Description

## Description

2026-09-21 03:39: `boop beep lane delete lab-scm-locals-vs-fast --dry-run` printed `removed branch lab/scm-locals-vs-fast` and `deleted lab-scm-locals-vs-fast`, and the worktree, branch, tmux session and route were all gone. The supervisor then reported `done rc=129 (killed by SIGHUP)`. The lane had zero files, so nothing was lost this time.

The agent-bus law says `--dry-run` first on every lane verb. This is the one verb where it lies.

## Acceptance Criteria
- [ ] `lane delete <lane> --dry-run` on a live lane prints what it would remove and removes nothing: worktree present, branch present, tmux session present, route row present
- [ ] a test in `crates/boop/tests/` proves it against a throwaway lane
- [ ] `boop --help` text for `lane delete` states the dry-run contract

## Tests Run

## Implementation Notes

## Comments

## Decisions
