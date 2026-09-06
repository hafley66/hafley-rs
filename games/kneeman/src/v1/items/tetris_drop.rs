//! TetrisDropper: TetrisGun's sibling (mod-api.md Tier 0 per-kind item file). Same held-tool
//! spec, same permanent stroke material (row 1 TETRIS), same tetromino table
//! (`stage::tetromino_path`/`tetromino_outline`) -- but the shot is a PURE VERTICAL drop spawned
//! in front of the fighter instead of an arc lob. `item.rs`'s `fire_gun` builds the spawn
//! geometry inline for its TetrisDropper branch (same house style as every other gun's fire
//! branch there) and calls `shape_from_aim_y` below for the piece pick; everything after spawn
//! (gravity, stacking, settle, bake into standable/strikeable terrain) is the SAME
//! `integrate_ink` path the arc shot rides in `stage/mod.rs` -- nothing here duplicates it.

use crate::v1::TETROMINO_SHAPES;
use crate::v1::items::behavior::{Attach, ItemBehavior};

// parity(v1-tetris-dropper): the sibling tool quantizes vertical aim into the shared tetromino table and spawns that permanent outline on a pure vertical drop instead of an arc
pub(crate) struct TetrisDropperKind;

// Identical classifier row to TetrisGunKind: TetrisDropper is the same held tool (counts
// toward the pickup cap, follows the hand, drops like a gun) -- only the fire-time trajectory
// differs, in item.rs's fire_gun. `despawn_when_spent`/`catchable` stay the trait default
// (false): a spent gun vanishes instantly in `fire_gun` (no idle unload), and a flying
// tetromino tool reads awkward to snatch, same read as the arc gun (tetris_gun.rs).
impl ItemBehavior for TetrisDropperKind {
    fn aims(&self) -> bool {
        true
    }
    fn attach(&self) -> Attach {
        Attach::Hand
    }
}

/// Piece pick from the main stick's `aim_y` at the fire press (`InputFrame.aim_y`, the existing
/// -1 up..+1 down field -- no new input state). Linear quantization across the tetromino table:
/// stick up picks the first shape, neutral the middle shape, down the last shape.
///
/// ```text
///   aim_y   -1.0  -0.5   0.0   0.5   1.0
///   shape    0     1      2     3     4     (TETROMINO_SHAPES == 5: I, O, T, L, S)
/// ```
///
/// `aim_y` is clamped first so an analog reading past +-1 can't index out of range.
pub(crate) fn shape_from_aim_y(aim_y: f32) -> u8 {
    let unit = (aim_y.clamp(-1.0, 1.0) + 1.0) * 0.5; // 0.0 (up) .. 1.0 (down)
    (unit * (TETROMINO_SHAPES - 1) as f32).round() as u8
}
