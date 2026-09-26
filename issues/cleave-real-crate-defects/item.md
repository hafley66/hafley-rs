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
- [x] cut includes contiguous leading doc comments and attributes
- [x] import rewrites keep the declaration's visibility prefix
- [x] fn-local `use` items are neither anchors nor treated as file imports
- [x] importers reaching the item through a re-export (hops > 0) are not rewritten; integration-test/example files spell the package ident, never `crate::`

## Comments

### 2026-09-25T17:53:01Z · @claude-m6

Found during batch A: when DEST already imports SRC#ITEM (DEST is a caller), the plan kept that import and added one naming DEST itself. Fixed: DEST's import block drops the item; callers skip DEST.

### 2026-09-25T18:15:43Z · @claude-m6

Batch A (rosters out of lang/mod.rs) found four more, all fixed: (1) qualified call sites crate::lang::rehome_for(..) were not callers (no import row); now read from resolved_edge sites and respelled. (2) pub use re-exports were planned as orphans and deleted. (3) a relative use child::X travelled as child::X, or via a private module path (prolog::_0_source); now respelled from SRC's module. (4) use a::b as c landed as use a::b::c.

### 2026-09-26T03:14:15Z · @claude-lane-w

Lane W (ryi/write-side a2698c8a): AC 1-3 were fixed by 39a24eac; AC 4 fixed (package-ident and nested direct importers are callers; relayed importers untouched). Also fixed: whole-file item not a declaration, trait import dropped when only a method call used it, pub use SRC::* glob gains pub use DEST::ITEM, indented use in mod tests respelled, serde(with) inline mod travels, use super::* inside an inline mod is not a file glob, DEST never imports itself. Pinned by tests/175_cleave_ladder.rs; a 10-item --list batch over a crates/soopy copy compiles (0 errors, 3 unused-import warnings). Open: item 5, multi-line lists still collapse when a name leaves them.


