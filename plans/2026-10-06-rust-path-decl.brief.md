# Brief: one rust-analyzer-backed #[path] spelling for move and cleave

Base: origin/main. Concern: the text of a new or rewritten `mod` declaration (`#[path = "..."] mod x;`)
in move and cleave. Today rust-analyzer Names owns module placement, but the declaration string is
built by hand in crates/sprefa-extract/src/edit/rust_mutate.rs (around :179-:210) and friends.
Defects from that code today: redundant #[path = "round.rs"]; numbered files reported undeclared;
move writes #[path = "edit/ts/rehome.rs"] where the ignored oracle
t_3_move_rust::moving_this_crates_own_module_leaves_it_compiling expects "ts/rehome.rs", plus an
unexpected `cross` declaration.

## Rule (Rust reference)
A #[path] on a module not inside an inline module block is relative to the directory of the declaring
file; inside inline `mod a { mod b; }` blocks it is relative to that file's directory plus the inline
module path (non-mod.rs files add the file stem). Emit #[path] only when the default location
(`<dir>/<name>.rs` or `<dir>/<name>/mod.rs`, per declaring-file kind) differs from the target file.

## Work, one commit each, failing test first
1. Table-driven fixture over declaration cases: mod.rs / lib.rs / non-mod-rs declarer, inline module
   nesting, numbered file (needs #[path]), plain name (no #[path]), cross-crate destination. Each
   row is checked by `cargo check` on the edited fixture, plus the exact declaration text.
2. One function computes the declaration from the declaring file and rust-analyzer's module tree;
   move, cleave and rename call it. Delete the hand-built path strings.
3. Run the two ignored oracles with --ignored (t_3_move_rust moving_this_crates_own_module_leaves_it_compiling,
   t_5_rename_rust self_rename_is_judged_by_rustc): both must pass, compiler included. If the
   oracle's expected text is wrong per cargo, update it and quote the cargo output in the commit.
4. Full suite once on the final commit (sprefa-extract cli,ts-checker; hafley_scm rust-checker),
   counts in the report; coordinator pushes from it.

## Limits
Test files under tests/ are also being edited by chore/test-shrink: add new tests as new fixture
dirs + one table test file; do not edit existing test files except the move/rename oracles.
KACHE_DISABLED=1. No merge, no push.
