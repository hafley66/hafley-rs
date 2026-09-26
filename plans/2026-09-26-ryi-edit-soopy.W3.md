# W3: ryi edit soopy batch defects

Base: `96292970`. Worktree: `feature/ryi-edit-soopy-bugs`.

## Rename

- Root cause: library and main roots tie for sibling Rust modules; `owning_root` picked main.
- Fix: prefer library root, preserve self-ownership of root files.
- Expected row: `rust_rename/reexport` two-row batch changes declarations, `pub use` bindings, and `crate` consumers in the hand-written `after` tree.
- Commit: `dfd51554`.

## Cleave

- Root cause: batch overlay applied raw duplicate respells; cleanup admitted malformed Rust candidates.
- Fix: normalize row edits and keep only parseable cleanup rewrites.
- Expected row: `cleave_ladder` Pattern then Counts batch leaves the Pattern source with only its new importer and `compile` function; the moved impl is present in the destination.
- Commits: `ede155a3`, `ae8c373e`.

## Gate

No cargo builds or tests and no ryi/codeql runs, per W3 instruction. Coordinator gate remains pending for both issues.
