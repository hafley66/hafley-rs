---
created: 2026-08-22
updated: 2026-09-27
type: improvement
status: needs-info
priority: normal
epic: harness-interface
related: ['@retire-tui-channels-codex-proxy']
labels: [domain-boop]
---

# Paste-into-pane path for instant after send_keys left boop-mux

## Description

PR #47 removed `Multiplexer::{send_keys_literal, send_text, send_key_named}` (plan §6: nothing types at a pane once `send_native` is gone). instant's `boop_mux_send_keys` is a user-facing paste into the TUI, so `paste_body` and `send_key` were re-implemented in `instant/src-tauri/src/0_tmux.rs:65-120` (tmux `load-buffer` + bracketed `paste-buffer`). Decide: restore the two methods on `Multiplexer` as an explicit user-paste API, or expose a paste verb from `Door`. Either way instant deletes its copy. Acceptance: one implementation of paste-into-pane across both repos.

## Decisions

### 2026-09-27T05:04:55Z · @codex

Current repro: boop-mux exposes no user paste methods; paste is implemented separately in boop cli/paste.rs and Instant 0_tmux.rs. Decision already requested: restore user paste methods on Multiplexer, or expose paste through Door?
