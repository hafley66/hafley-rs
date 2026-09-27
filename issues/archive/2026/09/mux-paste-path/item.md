---
created: 2026-08-22
updated: 2026-09-27
type: improvement
status: fixed
priority: normal
epic: harness-interface
related: ['@retire-tui-channels-codex-proxy']
labels: [domain-boop]
closed: 2026-09-27
---

# Paste-into-pane path for instant after send_keys left boop-mux

## Description

PR #47 removed `Multiplexer::{send_keys_literal, send_text, send_key_named}`. User-facing paste operations existed independently in `boop/src/cli/paste.rs` and `instant/src-tauri/src/0_tmux.rs`.

## Decision

Paste is a Multiplexer operation. `Door` remains delivery-only. The shared trait exposes named-key, literal-key, and bracketed text-paste methods, with one tmux implementation used by the Boop CLI and Instant.

## Acceptance Criteria

- [x] Restore user paste methods on `Multiplexer` and implement them once in `Tmux`.
- [x] Route Boop CLI paste/control/shout and Instant `boop_mux_send_keys` through `Multiplexer`.
- [x] Delete the duplicate tmux send and paste helpers from both callers.
- [x] Test bracketed multiline paste against a scratch `-L` server that the test kills.

## Tests Run

- [x] `cargo nextest run -p boop-mux -j 2 -E 'test(/user_paste/)'` (1 passed; scratch server cleanup)
- [x] `cargo nextest run -p boop -j 2 -E 'test(/paste/)'` (3 passed)
- [x] Instant companion commit: `c35ddb25`; `cargo check -p instant --lib -j 2` (passed with this worktree's local Boop crates)

## Decisions

### 2026-09-27T05:04:55Z · @codex

Repro: `Multiplexer` had no user paste methods; separate send/paste implementations existed in Boop and Instant. Resolved by the user: restore paste methods on `Multiplexer`; keep `Door` delivery-only. Verified by `user_paste_sends_text_through_a_scratch_tmux_server` and Boop paste tests.
