//! Lip projection (plans/ledge-domain.md, "ledges are ink command grabs"): the grabbable ledge
//! points, DERIVED per frame from the same geometry the sweeps read -- zero stored bytes, the
//! same doctrine as the `Surf` soup. A lip is `(world point, owner slot, node index, inward
//! face)`. The owner/node let a hang re-pin to the lip's CURRENT world point so a moving/rotating
//! stroke carries it (`world_pt` applies pos+rot); the two hardcoded stage lips ride through the
//! SAME projection as owner `-1` entries, so one consumer (the ledge-snap catch) serves both.
//!
//! Everything here is a stack temporary inside one `step()` call -- nothing is stored, serialized,
//! or checksummed (same as `Soup`). Rollback untouched.

use crate::v1::arena::InkNode;
use crate::v1::stage::{
    FLOOR_LEFT, FLOOR_RIGHT, GROUND_Y, InkPath, MAX_DRAWN, MOVER_SLOT, PILLAR_SLOT, SegClass,
};
use crate::v1::{Tune, Vector2};

/// One grabbable ledge point.
#[derive(Clone, Copy, Debug)]
pub struct Lip {
    /// World-space lip tip THIS frame.
    pub point: Vector2,
    /// `paths[]` slot the lip rides; `-1` = a hardcoded stage lip (fixed in world space).
    pub owner: i8,
    /// Node index within `paths[owner]` whose `world_pt` IS the tip -- the moving-body re-pin key.
    pub node: u8,
    /// The inward facing a catcher takes on the grab: the direction the floor extends from the tip
    /// (+1 = floor to the right, so grabbed from the left; -1 = floor to the left).
    pub face: f32,
}

impl Lip {
    const EMPTY: Self = Self {
        point: Vector2::ZERO,
        owner: -1,
        node: 0,
        face: 1.0,
    };
}

/// Fixed-cap stack buffer of the frame's lips: 2 stage lips + a generous margin of ink lips.
/// Overflow drops the excess (debug-asserted), same discipline as the surf `Soup`.
pub const MAX_LIPS: usize = 64;

pub struct LipSoup {
    buf: [Lip; MAX_LIPS],
    len: usize,
}

impl LipSoup {
    /// `t` gates the ink-derived lips by segment length (`ledge_min_len`, plans/ledge-ship-fixes.md
    /// #2) -- only the two hardcoded stage lips are exempt (their "segment" is the whole main
    /// floor, not a drawn stroke span).
    // parity(v1-ink-ledge-lifecycle): classified ink lips enter the same directional claim path as stage ledges and a live moving lip carries or drops its claimant as the source node changes
    pub fn collect(paths: &[InkPath; MAX_DRAWN], nodes: &[InkNode], t: &Tune) -> Self {
        let mut soup = LipSoup {
            buf: [Lip::EMPTY; MAX_LIPS],
            len: 0,
        };
        // the two hardcoded stage lips FIRST (owner -1), so a fighter near both a stage and an ink
        // lip catches the stage lip -- byte-identical priority to the old stage-only snap. Their
        // inward face points at the stage center (right lip faces left, left lip faces right).
        soup.push(Lip {
            point: Vector2::new(FLOOR_RIGHT, GROUND_Y),
            owner: -1,
            node: 0,
            face: -1.0,
        });
        soup.push(Lip {
            point: Vector2::new(FLOOR_LEFT, GROUND_Y),
            owner: -1,
            node: 0,
            face: 1.0,
        });
        // Player-drawn strokes (`owner >= 0`) contribute ink lips, PLUS the container hull: the
        // only SOLID baked fixture (the pillar/mover are soft and never grabbable). Its hatch-flank
        // lips flow through the SAME directional catch test as any other lip now (plans/
        // ledge-ship-fixes.md #3 -- the old `container`-gated `allow_container` special case is
        // gone; the directional box in `try_ledge_snap` separates entry from exit on its own). Its
        // `Lip.owner` is the live SHIP_SLOT, so a hang rides the flying hull like any ink lip.
        for (slot, path) in paths.iter().enumerate() {
            if !path.active() || path.drawing {
                continue;
            }
            if slot == PILLAR_SLOT || slot == MOVER_SLOT {
                continue; // baked simple fixtures (the wall pillar / the moving platform): their
                // flat ends or vertical face aren't meaningful grabbable lips. Slot-keyed
                // (not the old `owner<0 && !solid` proxy): the hull is also `solid=false`
                // now (2026-07-07 playtest, soft drop-through top), but its hatch lips ARE
                // grabbable, so the proxy would have silently retired them.
            }
            emit_path_lips(path, slot as i8, t.ledge_min_len, nodes, &mut |lip| {
                soup.push(lip)
            });
        }
        soup
    }

    pub fn lips(&self) -> &[Lip] {
        &self.buf[..self.len]
    }

    fn push(&mut self, lip: Lip) {
        debug_assert!(self.len < MAX_LIPS, "lip soup overflow");
        if self.len < MAX_LIPS {
            self.buf[self.len] = lip;
            self.len += 1;
        }
    }
}

/// Emit the outboard tips of every `Ledge`-classed segment of `path`, EXCEPT a segment shorter
/// than `min_len` px (plans/ledge-ship-fixes.md #2: "only when enough length" -- short scribbles
/// stop being grab magnets, only real edges are ledges). A `Ledge` segment is a Floor tip
/// (`classify`); an ENDPOINT of it is an outboard lip when no walkable segment continues past it
/// (an open end, or a neighbor that turns off the floor -- the classic drop-off corner). Both
/// endpoints can qualify (a floating bar has two grabbable ends). Inward `face` points from the tip
/// toward the OTHER endpoint of the segment (where the floor continues).
fn emit_path_lips(
    path: &InkPath,
    slot: i8,
    min_len: f32,
    nodes: &[InkNode],
    out: &mut impl FnMut(Lip),
) {
    let node_count = path.len as usize;
    let walkable = |class: SegClass| matches!(class, SegClass::Floor | SegClass::Ledge);
    for seg in 0..node_count.saturating_sub(1) {
        if path.seg_class(seg, nodes) != SegClass::Ledge {
            continue;
        }
        let (start_pt, end_pt) = path.world_seg(seg, nodes);
        if (end_pt - start_pt).length() < min_len {
            continue; // too short to read as a real edge -- no lip, either endpoint
        }
        // node `seg` is a tip when nothing walkable feeds into it from before (open start, or a
        // non-floor previous segment).
        let start_tip = seg == 0 || !walkable(path.seg_class(seg - 1, nodes));
        // node `seg + 1` is a tip when nothing walkable continues past it (open end, or the next
        // segment turns off the floor).
        let end_tip = seg + 1 >= node_count - 1 || !walkable(path.seg_class(seg + 1, nodes));
        if start_tip {
            out(Lip {
                point: start_pt,
                owner: slot,
                node: seg as u8,
                face: face_toward(end_pt.x - start_pt.x),
            });
        }
        if end_tip {
            out(Lip {
                point: end_pt,
                owner: slot,
                node: (seg + 1) as u8,
                face: face_toward(start_pt.x - end_pt.x),
            });
        }
    }
}

/// Inward facing from a signed x-delta toward the floor interior; never 0 (a real Floor is never
/// perfectly vertical), so a hang always has a definite facing.
fn face_toward(dx: f32) -> f32 {
    if dx >= 0.0 { 1.0 } else { -1.0 }
}

/// The CURRENT world point of a live ink lip, or `None` when the lip is gone: the path died, the
/// node decayed past `len`, or the segment reclassified off `Ledge`. The hang re-pin and the ink
/// climb both read this -- a `None` is the drop-to-Air signal. Stage lips (`ink < 0`) return `None`
/// here; their fixed world point is the caller's job.
pub(crate) fn lip_world(
    paths: &[InkPath; MAX_DRAWN],
    nodes: &[InkNode],
    ink: i8,
    node: u8,
) -> Option<Vector2> {
    if ink < 0 {
        return None;
    }
    let path = &paths[ink as usize];
    if !path.active() || path.drawing {
        return None;
    }
    let index = node as usize;
    if index >= path.len as usize {
        return None;
    }
    // a lip node is an endpoint of a Ledge segment: either segment `index` starts at it, or segment
    // `index - 1` ends at it. Neither still Ledge -> the lip is gone.
    let starts_here = path.seg_class(index, nodes) == SegClass::Ledge;
    let ends_here = index > 0 && path.seg_class(index - 1, nodes) == SegClass::Ledge;
    (starts_here || ends_here).then(|| path.world_pt(index, nodes))
}
