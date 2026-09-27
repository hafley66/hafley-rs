---
created: 2026-09-26
updated: 2026-09-26
type: bug
reporter: claude
status: fixed
priority: high
related: ['@cleave-cross-crate-reach']
labels: [extract]
---

# cleave into a path outside any crate rewrites callers to an undeclared crate:: module

## Description

Corpus: ascii-renderer at main `1c24a4b` (single binary crate, `src/main.rs` declares ~140 `mod` lines), worktree `.claude/worktrees/agent-abab70b186ac7c270`. `ryi 0.1.0` = `~/.cargo/bin/ryi`, sha256 `c2ec8bb01142049e…`, installed 2026-09-26 03:24 -0400 from worktree `.claude/worktrees/merge-main` (since removed). 18 commits touched `crates/sprefa-extract` after that build (latest `0d686c37`); not re-checked against main. Found while planning `plans/3_engine_crate_isolation.md` (branch `plan/engine-crate-isolation-v3`, `ae4a003`), which splits an engine library crate out of the binary. All runs are dry runs.

`ryi cleave --drag src/opts.rs#rand_knob crates/ascii-engine/src/_6_knobs.rs` exits 0 and plans an edit set that does not compile. `crates/ascii-engine/` does not exist and has no `Cargo.toml`, so DEST belongs to no package.

1. **Callers are rewritten to a module that is never declared.** For example, in `src/modes/_43_azulejo.rs`:
   ```
   -                .map(|p| crate::opts::rand_knob(s, p))
   +                .map(|p| crate::_6_knobs::rand_knob(s, p))
   ```
   No `mod _6_knobs;` is added to `src/main.rs`, and no `#[path]` either.
2. The same happens with `cleave --drag src/opts.rs#param_f32 crates/ascii-engine/src/_2_knobs.rs`. It prints `create  - -> crates/ascii-engine/src/_2_knobs.rs (465 bytes)` and `use crate::_2_knobs::param_f32;` in callers.

Expected: when DEST is under no package, or under a package other than SRC's, either stop with a named error (exit 2) or spell callers through the destination package (`ascii_engine::...`). If DEST is inside SRC's crate, add the `mod` line.

## Acceptance Criteria
- [x] cleave into a path with no declared Rust module path is a named stop (exit 2)
- [x] an undeclared destination with a usable parent gains its `mod` declaration; paths without a usable parent stop before caller rewrites
- [x] test: fixture binary crate, cleave into `crates/new/src/x.rs`, assert the stop

## Comments

### 2026-09-26T22:27:59Z · @claude

Re-checked with ryi built from origin/main 7c51f866 (2026-09-26 18:26): all 7 dry-run outputs identical to the 03:24 binary except stage hashes. Still reproduces.

### 2026-09-27 · @codex

Receipt: `t_166_cleave_rust` passed (18 tests), including `undeclared_destinations_stop_before_rewriting_callers` and `a_missing_destination_is_created_with_every_use_line`.

Follow-up receipt: package-nested `src/lib.rs` declarations are recognized; the combined `t_166_cleave_rust` and `t_173_move_cross_crate` targets passed (33 tests).
