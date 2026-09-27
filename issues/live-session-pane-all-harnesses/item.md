---
created: 2026-08-22
updated: 2026-09-26
type: improvement
status: done
priority: normal
epic: harness-interface
related: ['@live-sessions-doors']
labels: [domain-boop]
closed: 2026-09-26
---

# LiveSession.tmux_pane filled for codex and opencode, Registry fans out live_session_in_pane

## Description

Only `door/claude.rs:160` fills `LiveSession.tmux_pane`; `door/codex.rs:113` and `door/opencode.rs:174` hardcode `None`, kimi has no registry. `deliver_hail` therefore falls back to the `agent_live` row for every non-claude door, and instant keeps a `read_routes` fallback in `0_harness_store.rs:432`. Fill `tmux_pane` from the boop route registry inside each `LiveSessions` impl (or from the codex app-server client list when it exposes one), and add `Registry::live_session_in_pane(pane)` that asks every harness. Acceptance: `boop beep hail` to a codex TUI route resolves the pane without the `agent_live` fallback; instant drops its fallback.

## Resolution

### 2026-09-27T03:41:04Z · @issuectl

Boop session_in_pane_on_socket fans out through harness live registries and resolves the registered route; Codex route sessions carry route.tmux as tmux_pane. Instant now uses the harness lookup without the old read_routes fallback. cargo nextest run -p boop-harness -j 2 -E test(route_session_keeps_the_registered_codex_pane) passed.
