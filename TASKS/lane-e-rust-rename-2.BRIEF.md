# Lane E-2: rust rename seats, resume from the rescued WIP

Read `TASKS/lane-e-rust-rename.BRIEF.md` first; it is the whole spec (setup, files you own, the four gaps E.1 to E.4, the rules, verification, commit subjects, deliverable, laws). This file only says where the previous worker stopped.

## Where it stopped

The previous worker edited `crates/sprefa-extract/src/lang/rust_rename.rs` from 1157 to 2173 lines and then ended its turn with no fixtures, no new tests, no commits, no report. Its file is preserved at `TASKS/lane-e-wip-rust_rename.rs` (in your checkout).

Its own last notes, in order:
- "Compiles clean with no new warnings."
- "The shorthand byte law false-positives on use-trees (`use crate::{A, old}`). Structural fix: shorthand rides the FieldWalk `Owner{shorthand}` site instead of bytes."
- "All 7 existing rust rename tests green, macro fixture restored. Checking the other rust suites:"

What the WIP contains (grep it): `enum DeclKind { Item, Method, Field { owner }, Variant { owner } }` replacing `Decl.method`; `UseLeaf.block: Option<Span>`; `fn text_spellings` on `RustSource`; a `FieldWalk` for struct literal / pattern / `x.old` seats; `variant_leaf` / `owner_reach` for variant paths; a `#[path]` map in `Corpus::open`.

## Your first steps

1. `cp TASKS/lane-e-wip-rust_rename.rs crates/sprefa-extract/src/lang/rust_rename.rs`.
2. `cargo build --features cli --bin extract` (with `CARGO_TARGET_DIR` per the brief). Fix what does not compile; the WIP compiled at its author's last check.
3. `cargo test --features cli --test 5_rename_rust --test 3_move_rust --test 71_rust_paths --test golden_parity`. Every pre-existing test must stay green with the WIP in place before you add anything.
4. Read the WIP against the four gaps in the brief and list, in REPORT.md `## Rows`, which of E.1 to E.4 the WIP implements fully, partially, or not at all, with file:line.
5. Then the brief's "Verification, in order" from step 1 (fixtures) onward, and the brief's four commits. If the WIP already covers a commit's subject, commit the WIP under that subject.

## Laws

Everything in the parent brief. Plus: do not end your turn until REPORT.md is written and the commits exist. A turn that ends without them is the failure mode being fixed here. If blocked, write `## Blocked` with the exact error and commit what is green.
