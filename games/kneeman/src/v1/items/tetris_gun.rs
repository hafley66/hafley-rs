//! TetrisGun: lobs a whole closed tetromino ink body per shot (mod-api.md Tier 0
//! per-kind file). No Item projectile -- the piece claims a path slot and is the plan's
//! "fired ink" (`fire_gun`'s TetrisGun branch); no per-tick behavior lives here.
//! `update_items`'s generic held-tool arms cover follow/settle via `spec()` alone.

use crate::v1::items::behavior::{Attach, ItemBehavior};

// parity(v1-tetris-gun-lob): the held gun selects a tetromino outline and fires the whole permanent stroke as a spinning arced body through the shared ink flight path
pub(crate) struct TetrisGunKind;

// `despawn_when_spent`/`catchable` stay the trait default (false): a spent gun vanishes
// instantly in `fire_gun` (no idle unload), and it's an uncatchable hand-tool -- a flying
// tetromino gun reads awkward to snatch, and it doubles as the proof the per-kind gate works
// (grab_tests). TetrisDropper (its sibling, items/tetris_drop.rs) shares the same read.
impl ItemBehavior for TetrisGunKind {
    fn aims(&self) -> bool {
        true
    }
    fn attach(&self) -> Attach {
        Attach::Hand
    }
}
