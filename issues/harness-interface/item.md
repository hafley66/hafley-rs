---
created: 2026-08-22
updated: 2026-08-22
type: epic
owner: hafley66
status: open
priority: high
---

# Harness interface: HarnessId, Capabilities, LiveSessions, Door

## Description

## Description

One `Harness` object per agent CLI; a claude TUI hails a codex or opencode TUI and back; a fifth harness is one `impl` plus one enum variant. Plan with type signatures: `crates/boop/docs/plan-harness-interface-2026-08-22.md` (branch `refactor/harness-interface`, worktree `hafley-rs-worktrees/harness-interface`). Research: `crates/boop/docs/research-native-tui-control-2026-08-22.md`, review: `crates/boop/docs/review-2026-08-22.md`.

## Cards

| # | card | size | blocked_by |
|---|---|---|---|
| 1 | harness-id-capabilities | M | - |
| 2 | live-sessions-doors | M | 1 |
| 3 | mail-over-doors | M | 2 |
| 4 | retire-tui-channels-codex-proxy | M | 3 |
| 5 | instant-harness-store-dedupe | S | 2 |
| 6 | branch-worktree-cleanup | S | 4 |
