---
created: 2026-09-25
updated: 2026-09-25
type: bug
status: fixed
priority: normal
related: ['@ryi-cli-cleanup']
labels: [extract]
closed: 2026-09-25
---

# ryi cleave into a new Rust file never declares its module

## Description

## Description

`ryi cleave SRC#ITEM DEST` with a DEST that does not exist yet created the file and rewired the callers, but no module declared it: the crate stopped compiling (`E0432` on the caller's new `use crate::round::…`), and `--verify` rolled the run back.

Repro (fixtures/rust_cross, before the fix): `ryi cleave alpha/src/shapes.rs#perimeter alpha/src/round.rs --commit --verify "cargo check --offline -q"` -> `verify failed (rc=101): rolled back 2 files`.

## Fix

`Cleave::declare_new_file` (default None; Rust implements it): the file that owns DEST's directory (`dir/mod.rs`, `dir/lib.rs`, `dir/main.rs`, or `dir.rs`) gains `mod name;` after its last file-level `mod`. A numbered file (`4_round.rs`) is declared `#[path = "4_round.rs"] mod round;`, and `spell_module` spells it `round`. Visibility: `pub(crate)` within a crate, `pub` across crates.

## Acceptance Criteria
- [x] a cleave into a new file compiles (same crate and cross crate)
- [x] a numbered new file keeps its `#[path]` form

## Tests Run
- tests/173_move_cross_crate.rs

## Implementation Notes
Found by milestone M5 (cross-crate move) while checking cleave into hafley_scm-style numbered files.
