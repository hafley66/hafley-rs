# Extract rename rejects a bound Rust module rename at macro call sites

Date: 2026-09-11  
Extract: `0.1.0` from `/Users/chrishafley/.cargo/bin/extract`  
Extract source revision: `sprefa@71c1f8b0ad0d6f4dc7c7afc8a2de51976f2ab084`  
Consumer revision: `hafley-rs@ac6f70501d027a175a4c9bce7dcd11e1944d5045`

## Goal

Rename a Rust module and every reference bound to that module while leaving
same-spelled fields, locals, strings, trace targets and prose unchanged:

```rust
pub mod ground;
use crate::{Phase, air, ground};
ground::decide(...);

inventory.ground       // field, must stay ground
let ground = ...;      // local, must stay ground
"ground"                // string, must stay ground
```

The file move had already changed `1b_ground.rs` to `_1b_ground.rs`. The desired
module declaration was `pub mod _1b_ground;`, with bound paths rewritten to
`_1b_ground::...`.

## Reproduction

From `/Users/chrishafley/projects/hafley-rs`:

```sh
extract rename \
  games/crates/fighter/src/lib.rs#ground \
  _1b_ground \
  --root /Users/chrishafley/projects/hafley-rs \
  --state /private/tmp/fighter-extract-verify \
  --text-refs
```

Observed exit code: `6`.

Representative diagnostics:

```text
games/crates/fighter/src/_5_status.rs byte 6825: macro body reaches the symbol at runtime
games/crates/fighter/src/_5_status.rs byte 9289: macro body reaches the symbol at runtime
games/crates/fighter/tests/6_status.rs byte 792: macro body reaches the symbol at runtime
games/crates/fighter/tests/6_status.rs byte 3619: macro body reaches the symbol at runtime
```

The same diagnostics occur during a batch `--list` dry run.

## Actual references at the reported files

The files contain distinct bindings that syntax and name resolution must keep
separate:

```rust
use crate::{Phase, air, ground};
use game_fighter::{Phase, air, ground, status::runtime_inventory};

ground::Facts { ... }
ground::decide(...)
ground::Event::Motion(...)

inventory.ground
let ground = ground_transitions();
"ground"
assert_eq!(ground, 128, "ground {phase:?} {event}");
```

The bound module paths inside `assert_eq!` and other macro argument token trees
are executable Rust expressions. Treating the whole macro body as ambiguous
prevents the semantic rename even though the declaration and imported module
binding identify the target.

## Expected behavior

`extract rename` should:

1. Resolve `lib.rs#ground` to the module declaration.
2. Rename imports and paths bound to that module, including expression tokens
   passed through ordinary macros.
3. Preserve fields, locals, strings, trace targets and prose named `ground`.
4. Produce a complete dry-run plan and apply it with `--commit`.
5. Reserve exit code 6 for references whose binding cannot actually be
   distinguished after semantic analysis.

Expected examples:

```rust
pub mod _1b_ground;
use crate::{Phase, air, _1b_ground};
_1b_ground::decide(...);

inventory.ground
let ground = ground_transitions();
"ground"
```

## Secondary transactional problem

The attempted migration used individual `extract rename --commit` calls after
the batch dry run failed. The first two unambiguous module renames were applied,
then the third rename stopped at exit 6. The worktree therefore contains a
partially migrated module set.

For `--list --commit`, the required contract is atomic application of the full
validated list. For sequential single-renames, the caller must retain a
machine-readable rollback plan so a later ambiguity does not require manual
reconstruction.

## Regression fixture

Use one Rust crate containing:

```rust
pub mod ground;

macro_rules! check {
    ($value:expr) => { $value };
}

fn run(inventory: &Inventory) {
    let ground = "ground";
    let _ = check!(ground::decide());
    let _ = inventory.ground;
    let _ = ground;
}
```

Rename the declaration `ground` to `_1b_ground` and assert an inline snapshot
of the complete diff. The module declaration and `ground::decide` must change;
the field, local and string must remain byte-identical. Run the same fixture as
one row in a multi-rename `--list --commit` transaction and verify all-or-zero
application when another row contains a genuine ambiguity.

