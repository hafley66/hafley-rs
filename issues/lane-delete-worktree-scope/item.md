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

- [x] culprit named with the command text
- [x] bulk delete never removes a path that is not a listed route`s worktree_dir
- [x] sibling-preservation test
- [x] `--dry-run` on bulk delete lists paths

## Agent Runs

### 2026-08-25T13:15:46Z · @feat-epic-wave-b

910e778 CULPRIT: agent_cmd session 4799 (a sprefa perf lane), 2026-08-25 04:19:48 UTC / 00:19:48 local, command text: rm -rf ~/projects/hafley-rs/.boop-worktrees; ln -sfn /Users/chrishafley/projects/hafley-rs/.worktrees/origin-main /Users/chrishafley/projects/sprefa/.boop-worktrees/perf/hafley-rs; readlink ... . The same session ran ln -s /Users/chrishafley/projects/hafley-rs .boop-worktrees/perf/hafley-rs 15 s earlier, which resolved through an already-existing symlink and left the self-link the issue reports; the rm was its cleanup. boop is exonerated: lane delete <lane> and lane prune were both registry-only and touched no path, and lane delete --state dead routed to the registry-only run_prune. FIX: bulk delete now removes each dead route's own worktree_dir and only that. worktree::deletable_worktree canonicalizes the path and answers Some only for a linked git worktree whose own top level is itself, so the .boop-worktrees parent, the main checkout, and a plain directory are never candidates; removal is one git worktree remove --force from worktree_owner's repo. --dry-run prints every route and path and removes nothing. Tests: only_a_lanes_own_worktree_is_a_delete_candidate and a_bulk_delete_of_two_siblings_leaves_the_third_and_the_parent_alone (boop-harness 18 worktree tests pass). Live receipt in the scratch store: --dry-run listed 2 lanes / 2 worktrees and removed nothing, the real run removed exactly chore/door-claude and chore/kimi-terminal-receipt, and .boop-worktrees, .boop-worktrees/chore, feature/cx-a4 and feature/cx-b4 all survived.
