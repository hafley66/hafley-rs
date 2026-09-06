//! Roster row 1: the Falcon row (queue-2026-07-03 item 4), moved out of tune.rs so
//! "add a character" means "add a file" (plans/swordsman-lucas.md row 1).

use crate::v1::chars::kneeman;
use crate::v1::{CharSpec, SpecialMove};

/// KneeMan's kit, but the up-B (special slot 2) is the stationary command grab
/// (`SpecialMove::FALCON_DIVE`, `SpecialKind::DiveGrab`) instead of the default Rise
/// recovery. THIS is the char seam for the command grab -- the move is gated purely on
/// `specials[2].kind == DiveGrab`, which `char_id` selects via the roster row.
pub fn spec() -> CharSpec {
    let mut s = kneeman::spec();
    s.specials[2] = SpecialMove::FALCON_DIVE;
    s
}
