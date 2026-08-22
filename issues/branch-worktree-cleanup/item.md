---
created: 2026-08-22
updated: 2026-08-22
type: chore
status: open
priority: normal
epic: harness-interface
related: ['@retire-tui-channels-codex-proxy']
labels: [domain-boop, needs-chris]
size: S
---

# Delete superseded branches and worktrees, open the PR

## Description

## Description

After cards 1–4 land on main: 55 local branches, 0 GitHub PRs, 60 worktrees (review §1, §4). Delete the superseded set in review §4.2 (codex control ×3, pane liveness ×3, native-child ×3, tracing ×3, session-family ×3, `fix/codex-inspecting-proxy`, `backup/*`, `feature/boop-auto-sync`, `feature/boop-tell-parent`); `git worktree prune`; remove `/private/tmp/hafley-*` and `hafley-rs-worktrees/*` dirs for deleted branches. Rebase separately, they predate the `main.rs` → `cli/` split: `fix/boop-main-fixes`, `fix/boop-db-convoy`. Open a GitHub PR for `refactor/harness-interface` so review happens on a remote surface.

## Acceptance Criteria

- [ ] `git branch | wc -l` ≤ 15 (soopy ×9 untouched)
- [ ] `git worktree list | wc -l` ≤ 12
- [ ] one open PR: `refactor/harness-interface`
