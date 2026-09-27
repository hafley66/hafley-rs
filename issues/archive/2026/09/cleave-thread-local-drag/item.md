---
created: 2026-09-26
updated: 2026-09-26
type: bug
reporter: claude
status: fixed
priority: normal
related: ['@cleave-cross-crate-reach']
labels: [extract]
---

# thread_local! statics invisible to cleave; --drag leaves them unreported

## Description

Corpus: ascii-renderer at main `1c24a4b` (single binary crate, `src/main.rs` declares ~140 `mod` lines), worktree `.claude/worktrees/agent-abab70b186ac7c270`. `ryi 0.1.0` = `~/.cargo/bin/ryi`, sha256 `c2ec8bb01142049e…`, installed 2026-09-26 03:24 -0400 from worktree `.claude/worktrees/merge-main` (since removed). 18 commits touched `crates/sprefa-extract` after that build (latest `0d686c37`); not re-checked against main. Found while planning `plans/3_engine_crate_isolation.md` (branch `plan/engine-crate-isolation-v3`, `ae4a003`), which splits an engine library crate out of the binary. All runs are dry runs.

`src/opts.rs:55-57` declares a static inside `thread_local!`:
```rust
thread_local! {
    pub(crate) static LIVE_PARAMS: std::cell::RefCell<std::collections::BTreeMap<&'static str, Option<f32>>> = ...
}
```

1. `ryi cleave src/opts.rs#LIVE_PARAMS crates/ascii-engine/src/_2_knobs.rs` exits 2 with only `src/opts.rs declares no LIVE_PARAMS`.
2. `ryi cleave --drag src/opts.rs#param_f32 ...` exits 0 and prints `drag fixpoint 1 passes`. `param_f32` (`src/opts.rs:63`) reads `LIVE_PARAMS`, but the drag does not take it along and does not report it. The moved body still references `LIVE_PARAMS` in the new file (`+    match LIVE_PARAMS.with(...)`), where it does not resolve.

Compare `ryi cleave src/morph.rs#IterateFrameRenderer ...`, which prints `ungraded iterate_grid_into` and `next: ryi graph --uses iterate_grid_into <root>`. That output is the useful behaviour, although it also exits 0 with no plan written. Expected: a reference the drag cannot move is reported as `ungraded`, and the run exits non-zero when the plan would leave an unresolved name.

## Acceptance Criteria
- [x] item names declared inside `thread_local!` and static-like items in other macros are named as unsupported
- [x] `--drag` reports referenced unsupported items as `ungraded`
- [x] a plan with unresolved references in DEST exits non-zero

## Comments

### 2026-09-26T22:28:00Z · @claude

Re-checked with ryi built from origin/main 7c51f866 (2026-09-26 18:26): all 7 dry-run outputs identical to the 03:24 binary except stage hashes. Still reproduces.

### 2026-09-27 · @codex

Receipt: commit for `thread_local_static_is_named_unsupported_and_fails_the_plan` and `unsupported_macro_items_are_ungraded_and_fail_the_plan`; focused `t_166_cleave_rust` target passed (16 tests).
