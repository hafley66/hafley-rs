//! Cosmetic event ring: what kind of burst happened, where, and on which tick. Deterministic
//! (so it rolls back) but NOT folded into the net checksum — cosmetics can't desync a match, so
//! they don't get to fail one. Split out of `lib.rs` (R5).

use crate::v1::{SimState, Vector2};
use serde::{Deserialize, Serialize};

/// Cosmetic event slots in the ring. Small on purpose: an fx is a note to the renderer,
/// not gameplay.
pub const MAX_FX: usize = 16;

/// What kind of cosmetic burst happened. The shell picks the visual per kind — vector
/// scribbles today, PNG/AnimatedSprite2D placement later. Adding a kind is one enum row
/// plus one shell draw arm; the sim only ever records (kind, where, when).
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum FxKind {
    None,
    Explosion, // bomb/rocket blast, plasma burn
    Muzzle,    // arm-gun shot flash
    Transform, // AC core attach poof
    Fire, // Falcon command-grab boom -- dedicated cheesy fireball (appended at END: bincode is positional)
}

/// One cosmetic event: where and on which tick. The shell derives age (`state.tick - fx.tick`)
/// and draws anything young enough; stale slots just stop rendering — nothing expires them.
#[derive(Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fx {
    pub kind: FxKind,
    pub pos: Vector2,
    pub tick: u64,
}

impl Fx {
    pub const EMPTY: Self = Self {
        kind: FxKind::None,
        pos: Vector2::ZERO,
        tick: 0,
    };
}

/// Record a cosmetic event for the renderer. Ring overwrite, no failure mode. This is the
/// "tell godot where to put the pngs" seam: the pure scan step emits WHAT/WHERE/WHEN as
/// plain state; everything about how it looks lives outside the loop. Deterministic (event-
/// driven), so it rolls back like any state; deliberately NOT folded into the net checksum —
/// cosmetics can't desync a match, so they don't get to fail one.
pub(crate) fn push_fx(n: &mut SimState, kind: FxKind, pos: Vector2) {
    n.fx[n.fx_head as usize % MAX_FX] = Fx {
        kind,
        pos,
        tick: n.tick,
    };
    n.fx_head = (n.fx_head + 1) % MAX_FX as u8;
}
