---
created: 2026-08-22
updated: 2026-08-22
type: improvement
status: done
priority: normal
epic: harness-interface
related: ['@live-sessions-doors']
labels: [domain-boop, intent-implementation]
size: S
closed: 2026-08-22
---

# instant drops its HarnessStore copies for boop-harness

## Description

## Description

`instant/src-tauri/src/0_harness_store.rs:53` duplicates discovery with a second `HarnessStore` trait and four impls (`:224,319,400,536`). instant already links `boop-store` and `boop-mux` (`src-tauri/Cargo.toml`); link `boop-harness` and call `registry.get(id).live()` / `.transcripts()`; delete the local impls. Also retires `boop_mux_session` pane lookup (`instant/src-tauri/src/0_tmux.rs:31`) in favour of `LiveSession.tmux_pane`.

## Acceptance Criteria

- [ ] `0_harness_store.rs` impl blocks deleted; `cargo check` in `src-tauri` green
- [ ] turn-visibility right-click still resolves (test `matches inline code and bold beside punctuation…` stays green)

## Comments

### 2026-08-22T22:45:06Z · @fable

Landed as instant PR #15 (refactor/harness-store-dedupe, 10d84ca + a1a27b0): 0_harness_store.rs 678 -> 450, impl HarnessStore = 0, cargo test 62 pass, vitest turn-visibility 5/5. Gaps filed: @harness-session-by-id, @live-session-pane-all-harnesses, @mux-paste-path.
