---
created: 2026-09-26
updated: 2026-09-26
type: bug
reporter: claude
status: fixed
priority: high
related: ['@cleave-cross-crate-reach']
labels: [extract]
closed: 2026-09-26
commits:
- hash: 5fa64e40556fa04e8b45557166566fdc5ac29518
  summary: 'fix(sprefa-extract): honor Rust relocate module stems'
---

# move --relocate-mod still writes #[path]; cross-crate move silently uses #[path]

## Description

Corpus: ascii-renderer at main `1c24a4b` (single binary crate, `src/main.rs` declares ~140 `mod` lines), worktree `.claude/worktrees/agent-abab70b186ac7c270`. `ryi 0.1.0` = `~/.cargo/bin/ryi`, sha256 `c2ec8bb01142049e…`, installed 2026-09-26 03:24 -0400 from worktree `.claude/worktrees/merge-main` (since removed). 18 commits touched `crates/sprefa-extract` after that build (latest `0d686c37`); not re-checked against main. Found while planning `plans/3_engine_crate_isolation.md` (branch `plan/engine-crate-isolation-v3`, `ae4a003`), which splits an engine library crate out of the binary. All runs are dry runs.

**`move --relocate-mod` still writes `#[path]`.** `ryi move --relocate-mod src/types.rs src/_0_types.rs` plans this edit, exit 0:
```
-mod types;
+#[path = "_0_types.rs"] mod types;
```
The help for `--relocate-mod` says "Move a Rust module's `mod` line instead of adding #[path]". Expected: `mod types;` becomes `mod _0_types;`, and every `crate::types::` path is respelled `crate::_0_types::`. The repo's numeric file prefix convention (`_0_name.rs`) depends on this. The result is the same with and without the flag.

**Moving into another crate also uses `#[path]`.** `ryi move src/types.rs crates/ascii-engine/src/_0_types.rs` plans:
```
-mod types;
+#[path = "../crates/ascii-engine/src/_0_types.rs"] mod types;
```
The file stays compiled into the binary crate through a path reaching outside `src/`. Expected: detect that DEST is outside SRC's package root and stop (exit 2), or require an explicit flag.

## Acceptance Criteria
- [x] `--relocate-mod` renames the `mod` item to the new stem and respells all `crate::<old>::` paths, with no `#[path]`
- [x] move with DEST outside SRC's package root is a named stop unless a flag opts into `#[path]`
- [ ] tests for both

## Comments

### 2026-09-26T22:27:59Z · @claude

Re-checked with ryi built from origin/main 7c51f866 (2026-09-26 18:26): all 7 dry-run outputs identical to the 03:24 binary except stage hashes. Still reproduces.

### 2026-09-27T02:17:58Z · @codex

Current ryii reproduced both defects; relocation now renames the module and stops outside the crate root. t_3_move_rust passed (19 tests), including cargo check of the renamed fixture.

