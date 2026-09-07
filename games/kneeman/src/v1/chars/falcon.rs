//! Roster row 1: the Falcon row (queue-2026-07-03 item 4), moved out of tune.rs so
//! "add a character" means "add a file" (plans/swordsman-lucas.md row 1).

use crate::v1::chars::kneeman;
use crate::v1::{CharSpec, SpecialMove};

/// KneeMan's kit with DiveGrab up-special and jump-restoring down-special.
/// Kick uses separate ground/air damage phases with authored timing, geometry and angle-361 approximation.
pub fn spec() -> CharSpec {
    let mut s = kneeman::spec();
    s.specials[2] = SpecialMove::FALCON_DIVE;
    s.specials[3] = SpecialMove::FALCON_KICK;
    s
}
