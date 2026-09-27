---
created: 2026-08-22
updated: 2026-08-22
type: epic
owner: hafley66
status: obsolete
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

## Resolution

Repro receipt (2026-09-27): `cargo nextest run -p boop-proc -j 2 -E 'test(/a_claude_coordinator_takes_its_row_at_the_door_with_no_hooks_installed/)'` passes; the current registry dispatches through the recipient's `Harness::door()`, contains the five built-in adapters, and the planned identity/capability, live-session, and door cards are already landed. The remaining `Spawner` / `TranscriptSource` extraction is a method-location refactor with no current harness behavior failure to reproduce.
