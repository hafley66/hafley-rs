//! The live/toggleable blast zone: the frame's effective fighter KO rect (`ZoneRect`, resolved
//! once per `step()` from `Tune::zone_mode`), the zone-maker ink bounding box that can extend it, and the
//! per-item-kind exemption lookup. Split from stage/item so each stays under its line ratchet.

use crate::v1::{BLAST_LEFT, BLAST_RIGHT, BLAST_TOP, BLAST_Y, InkPath, MAX_DRAWN};
use crate::v1::{InkNode, ItemKind, Tune, Vector2};

/// The frame's effective FIGHTER blast rect (the live/toggleable blast zone). Resolved once per
/// `step()` from `Tune::zone_mode`; `None` there is `ZoneMode::Off` (fighters never KO at the
/// edges this frame). Items are UNAFFECTED by any of this: they always read the static
/// `out_of_bounds` frame regardless of mode (see the despawn sites in item.rs/items/*.rs).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ZoneRect {
    pub lo: Vector2,
    pub hi: Vector2,
}

impl ZoneRect {
    /// The static `BLAST_*` frame as a rect: `ZoneMode::Static`'s value, and the floor
    /// `ZoneMode::InkExtends` never shrinks below.
    pub const STATIC: Self = Self {
        lo: Vector2::new(BLAST_LEFT, BLAST_TOP),
        hi: Vector2::new(BLAST_RIGHT, BLAST_Y),
    };

    /// `ZoneMode::InkExtends`: the static frame unioned with the live zone-maker ink bounding box
    /// (`ink_blast_zone`), if any zone ink is down.
    pub fn extended(ink: Option<(Vector2, Vector2)>) -> Self {
        match ink {
            None => Self::STATIC,
            Some((lo, hi)) => Self {
                lo: Self::STATIC.lo.min(lo),
                hi: Self::STATIC.hi.max(hi),
            },
        }
    }
}

/// True when `p` has crossed any edge of the given fighter zone rect -- the live/toggleable
/// counterpart to `out_of_bounds`'s static frame.
#[inline]
pub(crate) fn out_of_zone(p: Vector2, z: &ZoneRect) -> bool {
    p.x < z.lo.x || p.x > z.hi.x || p.y < z.lo.y || p.y > z.hi.y
}

/// The live blast zone: bounding box (min, max corner) of every STILL, zone-material player stroke.
/// `None` when no zone ink exists — callers fall back to the static `BLAST_*` frame.
/// (Baked strokes are excluded: a 2026-07-04 experiment let the zone-material hull carry the
/// rect in flight; the playtest verdict was "wonky", so the blast frame is hard walls the
/// ship bounces off instead -- see `integrate_ink`.)
// parity(v1-ink-zone-extension): only still live player strokes using the zone material extend the fighter blast rectangle; baked and traveling ink do not
pub fn ink_blast_zone(
    paths: &[InkPath; MAX_DRAWN],
    nodes: &[InkNode],
) -> Option<(Vector2, Vector2)> {
    let mut zone: Option<(Vector2, Vector2)> = None;
    for p in paths {
        if !p.active() || p.owner < 0 || p.mass <= 0.0 || p.traveling() || !p.props.zone {
            continue;
        }
        for i in 0..p.len as usize {
            let w = p.world_pt(i, nodes);
            zone = Some(match zone {
                None => (w, w),
                Some((lo, hi)) => (lo.min(w), hi.max(w)),
            });
        }
    }
    zone
}

/// Resolve an item kind's `ItemConfig.zone_exempt` flag for the quiet-despawn sites (mirrors
/// `fire_gun`'s gun->cfg match: same kind groupings share a config row). Kinds with no config row
/// (Pen/InkGun/badges) are never exempt -- only weapon rounds have a `zone_exempt` flag today.
pub(crate) fn item_zone_exempt(kind: ItemKind, t: &Tune) -> bool {
    match kind {
        ItemKind::BobGun | ItemKind::Bomb | ItemKind::Rocket => t.bomb.zone_exempt,
        ItemKind::TetrisGun | ItemKind::TetrisDropper => t.tetris.zone_exempt,
        ItemKind::PlasmaBall | ItemKind::AcCore => t.plasma.zone_exempt,
        ItemKind::LaserGun | ItemKind::LaserBolt => t.laser.zone_exempt,
        _ => false,
    }
}
