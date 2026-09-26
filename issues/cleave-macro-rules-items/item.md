---
created: 2026-09-26
updated: 2026-09-26
type: bug
reporter: claude
status: open
priority: normal
related: ['@cleave-cross-crate-reach']
labels: [extract]
---

# cleave cannot select macro_rules! items (declares no param)

## Description

Corpus: ascii-renderer at main `1c24a4b` (single binary crate, `src/main.rs` declares ~140 `mod` lines), worktree `.claude/worktrees/agent-abab70b186ac7c270`. `ryi 0.1.0`. Found while planning `plans/3_engine_crate_isolation.md` (branch `plan/engine-crate-isolation-v3`, `ae4a003`), which splits an engine library crate out of the binary. All runs are dry runs.

`ryi cleave src/registry.rs#param crates/ascii-engine/src/_1_mode.rs` exits 2 with only:
```
src/registry.rs declares no param
```
`param` is a `#[macro_export]`/`macro_rules!` macro in `src/registry.rs` (the `param!` knob macro every mode uses). cleave's item index does not include `macro_rules!` definitions.

Expected: `macro_rules!` items can be cleaved, keeping their `#[macro_export]`/`#[macro_use]` attributes. Callers depend on textual order and `#[macro_use] mod`, so the plan must also move or keep the `#[macro_use]` on the destination `mod` line. If this is out of scope, the error should say "macro_rules! items are not supported". "declares no param" reads as a typo in the selector.

## Acceptance Criteria
- [ ] cleave of a `macro_rules!` item moves it with its attributes and fixes `#[macro_use]` ordering, or stops with a message naming the unsupported item kind
