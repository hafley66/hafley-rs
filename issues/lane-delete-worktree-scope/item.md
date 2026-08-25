---
created: 2026-08-25
updated: 2026-08-25
type: bug
status: open
priority: high
epic: boop-one-path
labels: [domain-boop]
---

# Something removed .boop-worktrees wholesale under live lanes

## Description

## Description

Twice on 2026-08-25 (~00:19 and later during the one-sqlite-mailbox run) `hafley-rs/.boop-worktrees/` was removed wholesale while three lanes had live worktrees under it. Both opus lanes lost uncommitted work and rebuilt from committed patches. Candidates: `boop beep lane delete --state dead` walking dead routes whose `worktree_dir` sits under the same parent and removing the parent when it thinks it is empty; a lane cleanup script running `git worktree prune` plus `rm -rf .boop-worktrees`; the e2e cleanup in the codex-native-messaging run. Also seen: a self-symlink `hafley-rs -> hafley-rs` created in the repo root at 00:19.

Requirements: `lane delete` removes exactly the route`s own `worktree_dir` and nothing above it; a dry-run print of every path a bulk delete will remove; a test that a bulk delete with two dead routes under one parent leaves a third live sibling untouched; find the actual culprit from `boop db command` / shell history around 00:19 and name it here.

## Acceptance Criteria

- [ ] culprit named with the command text
- [ ] bulk delete never removes a path that is not a listed route`s worktree_dir
- [ ] sibling-preservation test
- [ ] `--dry-run` on bulk delete lists paths
