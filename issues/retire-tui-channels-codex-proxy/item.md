---
created: 2026-08-22
updated: 2026-08-22
type: chore
status: open
priority: normal
epic: harness-interface
related: ['@mail-over-doors']
labels: [domain-boop, intent-implementation]
size: M
---

# Delete tui/opencode/kimi channels and the codex InspectingProxy

## Description

## Description

Delete `boop-acp/src/channel/tui.rs` (864 lines), `channel/opencode.rs`, `channel/kimi.rs` (509), and `channel/codex.rs::InspectingProxy`; `boop codex` launches the TUI with `--remote` straight at the daemon and reads the thread id from `state_5.sqlite`. Re-does `feature/acp-all-harnesses` (`1fbc69e`) on the new trait; that branch conflicts with the `cli/` split and is deleted after. Today's proxy fixes `1a100ee`, `b20c18c` become dead code here. Lane P3, sonnet high.

## Acceptance Criteria

- [ ] `LaneChannel` impls: `AcpChannel` plus test fakes only
- [ ] `/resume` inside `boop codex` still works (probe 2026-08-22 receipt: 50 sessions listed)
- [ ] audit finding #12 closed
