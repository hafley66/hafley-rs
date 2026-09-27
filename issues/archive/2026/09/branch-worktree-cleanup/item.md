---
created: 2026-08-22
updated: 2026-09-26
type: chore
status: obsolete
priority: normal
epic: harness-interface
related: ['@retire-tui-channels-codex-proxy']
labels: [domain-boop, needs-chris]
size: S
closed: 2026-09-26
---

# Delete superseded branches and worktrees, open the PR

## Description

## Description

After cards 1–4 land on main. Counts re-measured 2026-08-22 evening: 80 local branches, 61 worktrees, 46 GitHub PRs all merged, 0 open (review §1 and §4 said 55 / 60 / 0). Delete the superseded set in review §4.2 (codex control ×3, pane liveness ×3, native-child ×3, tracing ×3, session-family ×3, `fix/codex-inspecting-proxy`, `backup/*`, `feature/boop-auto-sync`, `feature/boop-tell-parent`); `git worktree prune`; remove `/private/tmp/hafley-*` and `hafley-rs-worktrees/*` dirs for deleted branches. Rebase separately, they predate the `main.rs` → `cli/` split: `fix/boop-main-fixes`, `fix/boop-db-convoy`. Open a GitHub PR for `refactor/harness-interface` so review happens on a remote surface.

## Acceptance Criteria

- [ ] `git branch | wc -l` ≤ 15 (soopy ×9 untouched)
- [ ] `git worktree list | wc -l` ≤ 12
- [ ] one open PR: `refactor/harness-interface`

## Resolution

### 2026-09-27T03:22:38Z · @issuectl

Repro receipt (2026-09-26): PR #47 is already merged; the named stale refs were absent, merged, or removed, while current inventory is 313 local branches and 23 worktrees, so the August ≤15/≤12 caps no longer describe this cleanup set.
