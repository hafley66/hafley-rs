---
created: 2026-09-14
updated: 2026-09-14
type: bug
status: fixed
priority: normal
closed: 2026-09-14
closed_by: codex
commits:
- hash: 63eb9151
  summary: Report harness silence and idle retirement to parents
---

# Notify parent when delegated work stops progressing

## Description

User reports having to manually ask for progress after rustdoc worker stopped and route disappeared. Audit existing supervisor warning/idle retirement/follow-up handoff paths. Reuse existing no-progress warning work. Require automatic actionable deduplicated parent notices, preserved resumable identity, no loss of accepted follow-up, and explicit distinction between turn completion, idle retirement, and task completion. Worker fix-stalled-parent-status, base161f13a; parent owns validation/install/integration.

## Resolution

### 2026-09-14T11:37:29Z · @codex

Merged and pushed as 63eb9151; installed signed release boop 0.0.10 (241a2843). Running lanes warn once after 300 seconds of harness-channel silence (BOOP_PROGRESS_WARNING_SECS, 0 disables), report a newer channel write as active, and notify retirement per result episode. Parked mail is drained before the retirement deadline check. Validation: boop-proc 171 and boop-store 192 library tests passed with inherited TMUX/TMUX_PANE/BOOP_MAIL_DIR/BOOP_DB removed; real llmock plus coordinator/lane TUI lifecycle matrix 18/18 passed across Claude/Codex/OpenCode, no skips; no-default-features cargo check and release build passed. Claude replay exposes no mid-turn channel write, so its test asserts the quiet warning and no claimed recovery. Existing running supervisors retain their loaded executable until normal exit/revival. Logs: /private/tmp/boop-stall-lifecycle-final.log and /private/tmp/boop-stall-lib-isolated.log.
