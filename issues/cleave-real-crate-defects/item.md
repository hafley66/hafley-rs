---
created: 2026-09-25
updated: 2026-09-25
type: bug
status: open
priority: normal
labels: [extract]
---

# cleave on a real crate: lost docs, dropped pub, nested use anchor, tests spelled crate::

## Description

## Description
First real-crate cleave (M6): `ryi cleave crates/sprefa-extract/src/types.rs#ImportRef crates/sprefa-extract/src/0_edit_seams.rs` (dry run) planned five wrong edits:

1. The item's doc comment and attributes stay behind in SRC (the cut starts at `pub struct`, not at the `///` / `#[derive]` lines above it).
2. `lib.rs` `pub use types::{..., ImportRef, ...}` is rewritten as private `use types::{...}` (visibility dropped by `drop_leaf` / `use_line`).
3. The kept-import `use crate::edit_seams::ImportRef;` is inserted inside a function body in types.rs: the anchor is the last `use` anywhere, including fn-local `use` items.
4. `tests/5_move_scip.rs` (an integration-test crate) is rewritten to `use crate::edit_seams::ImportRef`; `crate::` there names the test crate. It imported through the lib's `pub use` re-export, which stays valid, so it needed no edit.
5. Multi-line `use` lists collapse to one line (cosmetic).

## Acceptance Criteria
- [ ] cut includes contiguous leading doc comments and attributes
- [ ] import rewrites keep the declaration's visibility prefix
- [ ] fn-local `use` items are neither anchors nor treated as file imports
- [ ] importers reaching the item through a re-export (hops > 0) are not rewritten; integration-test/example files spell the package ident, never `crate::`
