---
created: 2026-08-22
updated: 2026-08-22
type: improvement
status: open
priority: normal
epic: harness-interface
related: ['@live-sessions-doors']
labels: [domain-boop]
---

# LiveSession.tmux_pane filled for codex and opencode, Registry fans out live_session_in_pane

## Description

Only `door/claude.rs:160` fills `LiveSession.tmux_pane`; `door/codex.rs:113` and `door/opencode.rs:174` hardcode `None`, kimi has no registry. `deliver_hail` therefore falls back to the `agent_live` row for every non-claude door, and instant keeps a `read_routes` fallback in `0_harness_store.rs:432`. Fill `tmux_pane` from the boop route registry inside each `LiveSessions` impl (or from the codex app-server client list when it exposes one), and add `Registry::live_session_in_pane(pane)` that asks every harness. Acceptance: `boop beep hail` to a codex TUI route resolves the pane without the `agent_live` fallback; instant drops its fallback.
