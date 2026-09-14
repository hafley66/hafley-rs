# Boop worktree cleanup, 2026-09-14

Removed 39 old hafley-rs worktrees and their local branches. Across both repositories: 57 worktrees, plus the two temporary audit workers. Three stale historical worker routes were removed.

No historical product patch was merged during cleanup. Main has concurrent work from other sessions; it was left untouched. The ghcache integration worktree, unrelated labs and shared build caches were excluded.

## Retained work

The following source remains pending behind explicit issues. Archive refs are local Git tags; they were not published. WIP refs contain complete archival commits including formerly untracked source, with no test-pass claim.

| Feature | Issue | Source ref |
| --- | --- | --- |
| Resident ACP host | [acp-one-send-path](../issues/acp-one-send-path/item.md) | `archive/boop-cleanup-20260914/feature/boop-acp-host` |
| Expiring reminders | [boop-reminder-scheduling](../issues/boop-reminder-scheduling/item.md) | `archive/boop-cleanup-20260914/feature/boop-reminders-instant` |
| Typed lane status | [lane-status-command](../issues/lane-status-command/item.md) | `archive/boop-cleanup-20260914/feature/lane-status-command-terra` |
| TUI revival picker | [boop-revive-picker](../issues/boop-revive-picker/item.md) | `archive/boop-cleanup-20260914/wip/feature/tui-revive` |
| Lane table layout | [boop-lane-table-layout](../issues/boop-lane-table-layout/item.md) | `archive/boop-cleanup-20260914/wip/fix/lane-list-liveness` |
| Invocation build identity | [boop-invocation-build-identity](../issues/boop-invocation-build-identity/item.md) | `archive/boop-cleanup-20260914/wip/fix/f41-boop-invocation-build-id` |

## Removed checkouts

Shipped or superseded variants were removed after ancestry, patch-equivalence or source review. Historical variants that were not proved identical retain their exact source tags; removal does not claim that every hunk is already shipped.

| Old branch | Preserved source | Disposition |
| --- | --- | --- |
| `fix/fork-interactive` | `ea942ffa2b2fbfde5e2196f78bceaeec3147a22b` | shipped or superseded |
| `feature/boop-commit-push` | `5602f812349e8f16cc0f8079283c89f35a75a0b3` | shipped or superseded |
| `feature/boop-ps` | `9ca41fed4e4a2ed8b438baacbefe4bd6024b7fd7` | shipped or superseded |
| `fix/boop-selection-backend` | `ea942ffa2b2fbfde5e2196f78bceaeec3147a22b` | shipped or superseded |
| `fix/boop-fixture-lanes-purge` | `archive/boop-cleanup-20260914/fix/boop-fixture-lanes-purge` | shipped or superseded |
| `fix/boop-harness-model-spec-2` | `archive/boop-cleanup-20260914/fix/boop-harness-model-spec-2` | shipped or superseded |
| `feature/boop-parent-death` | `archive/boop-cleanup-20260914/feature/boop-parent-death` | shipped or superseded |
| `feature/boop-session-mood` | `archive/boop-cleanup-20260914/feature/boop-session-mood` | shipped or superseded |
| `feature/boop-tell-parent-2` | `archive/boop-cleanup-20260914/feature/boop-tell-parent-2` | shipped or superseded |
| `feature/boop-quiet-yields` | `archive/boop-cleanup-20260914/feature/boop-quiet-yields` | shipped or superseded |
| `feature/f41-boop-live-harness` | `archive/boop-cleanup-20260914/feature/f41-boop-live-harness` | shipped or superseded |
| `fix/f41-claude-delivery` | `archive/boop-cleanup-20260914/fix/f41-claude-delivery` | shipped or superseded |
| `fix/boop-selection-reviewed` | `archive/boop-cleanup-20260914/fix/boop-selection-reviewed` | shipped or superseded |
| `fix/boop-selection` | `archive/boop-cleanup-20260914/fix/boop-selection` | superseded |
| `fix/boop-doa-carcass` | `archive/boop-cleanup-20260914/fix/boop-doa-carcass` | superseded |
| `docs/boop-help-sweep` | `archive/boop-cleanup-20260914/docs/boop-help-sweep` | uncertain |
| `fix/boop-spawn-guards-2` | `archive/boop-cleanup-20260914/fix/boop-spawn-guards-2` | uncertain |
| `feature/boop-start-warm-detect` | `archive/boop-cleanup-20260914/feature/boop-start-warm-detect` | superseded |
| `feature/dl6-boop-concatmap-golden` | `archive/boop-cleanup-20260914/feature/dl6-boop-concatmap-golden` | uncertain |
| `feature/lane-tracing-events-json` | `archive/boop-cleanup-20260914/feature/lane-tracing-events-json` | uncertain |
| `chore/harness-replay-research` | `archive/boop-cleanup-20260914/chore/harness-replay-research` | superseded |
| `feature/f41-adapter-replay` | `archive/boop-cleanup-20260914/feature/f41-adapter-replay` | superseded |
| `feature/replay-pipes-f41` | `archive/boop-cleanup-20260914/feature/replay-pipes-f41` | superseded |
| `fix/boop-door-backoff` | `archive/boop-cleanup-20260914/fix/boop-door-backoff` | superseded |
| `fix/boop-door-backoff-r2` | `archive/boop-cleanup-20260914/fix/boop-door-backoff-r2` | superseded |
| `fix/boop-lane-lifecycle-r2` | `archive/boop-cleanup-20260914/fix/boop-lane-lifecycle-r2` | superseded-shared-mock-tui-terminal-env |
| `fix/boop-worktree-reclaim` | `archive/boop-cleanup-20260914/fix/boop-worktree-reclaim` | superseded |
| `fix/delivery-e2e` | `archive/boop-cleanup-20260914/fix/delivery-e2e` | superseded |
| `fix/f41-native-model-replay` | `archive/boop-cleanup-20260914/fix/f41-native-model-replay` | superseded |
| `fix/f41-visible-tui-20260912` | `archive/boop-cleanup-20260914/fix/f41-visible-tui-20260912` | uncertain |
| `fix/invocation-review` | `archive/boop-cleanup-20260914/fix/invocation-review` | superseded |
| `fix/luna-visible-tui-20260912` | `archive/boop-cleanup-20260914/fix/luna-visible-tui-20260912` | uncertain |
| `feature/tui-revive` | `archive/boop-cleanup-20260914/wip/feature/tui-revive` | kept-pending-feature |
| `fix/lane-list-liveness` | `archive/boop-cleanup-20260914/wip/fix/lane-list-liveness` | kept-pending-feature |
| `fix/f41-boop-invocation-build-id` | `archive/boop-cleanup-20260914/wip/fix/f41-boop-invocation-build-id` | kept-pending-feature |
| `feature/boop-acp-host` | `archive/boop-cleanup-20260914/feature/boop-acp-host` | kept-pending-feature |
| `feature/lane-status-command-terra` | `archive/boop-cleanup-20260914/feature/lane-status-command-terra` | kept-pending-feature |
| `feature/boop-reminders-instant` | `archive/boop-cleanup-20260914/feature/boop-reminders-instant` | kept-pending-feature |
| `fix/f41-analytics-finish` | `archive/boop-cleanup-20260914/fix/f41-analytics-finish` | superseded |

## Recovery and verification

Local archive: `/Users/chrishafley/projects/boop-cleanup-20260914/`. Its README explains recovery; the action ledger records exact heads, refs, dirty snapshots and removal evidence. The archive contains source and metadata, with no private Boop database copy.

Verification checked all 57 removed paths and branch refs, plus all 46 archive refs in the action ledger. No product tests were run during this source-and-Git cleanup. The npm cache pin, turn-lineage/composer exclusion, and favorite-note creation were confirmed in current shared implementations and were not reintroduced from old worktrees.
