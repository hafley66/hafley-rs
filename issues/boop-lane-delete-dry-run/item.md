---
created: 2026-09-20
updated: 2026-09-26
type: bug
status: fixed
priority: normal
labels: [boop]
closed: 2026-09-26
commits:
- hash: 8e660698
  summary: 'fix(boop): honor lane delete dry-run'
---

# boop: lane delete --dry-run deletes the lane

## Description

## Description

2026-09-21 03:39: `boop beep lane delete lab-scm-locals-vs-fast --dry-run` printed `removed branch lab/scm-locals-vs-fast` and `deleted lab-scm-locals-vs-fast`, and the worktree, branch, tmux session and route were all gone. The supervisor then reported `done rc=129 (killed by SIGHUP)`. The lane had zero files, so nothing was lost this time.

The agent-bus law says `--dry-run` first on every lane verb. This is the one verb where it lies.

## Acceptance Criteria
- [x] `lane delete <lane> --dry-run` on a live lane prints what it would remove and removes nothing: worktree present, branch present, tmux session present, route row present
- [x] a test in `crates/boop/tests/` proves it against a throwaway lane
- [x] `boop --help` text for `lane delete` states the dry-run contract

## Tests Run

## Implementation Notes

## Comments

## Decisions

## Resolution

### 2026-09-27T03:05:05Z · @issuectl

Receipt: 8e660698; `lane_delete_dry_run::single_lane_dry_run_preserves_a_live_route_and_its_pane` passed with a throwaway live tmux session, branch, worktree and route.
