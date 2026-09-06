//! The terrain bus (plans/body-bus.md §1): everything that contributes surfaces emits
//! `Surf` rows via `Randall`; every falling/flying body resolves against ONE soup via
//! the sweep helpers; and the any-surf resource refresh (`touch_refresh`) lives at this
//! single choke point. A wall is a floor with a different orientation and worse
//! friction — one definition, never a bespoke branch per surface owner.
//!
//! Everything here is a stack temporary inside one `step()` call: nothing is stored,
//! serialized, or checksummed. Rollback untouched.

use crate::v1::arena::InkNode;
use crate::v1::geo::{Geometry, Iso, NaiveGeom, Shape}; // `NaiveGeom::cast_shapes` (sweep_walls' swept arm)
use crate::v1::stage::{
    FLOOR_LEFT, FLOOR_RIGHT, GROUND_Y, InkPath, MAX_DRAWN, PLATFORMS, Platform, SHIP_SLOT,
    STAGE_BOTTOM, SegClass,
};
use crate::v1::{Badge, DT, FPS, Fighter, InputFrame, Tune, Vector2, WALK_THRESH, geo};

/// Contact generation (plans/body-unify.md step 1) + materials (step 2): the one
/// `Contact` dialect every collision pair emits, and the `Material` rows the generic
/// solve reads. Lives next to `collide()` -- the solve those contacts feed.
pub mod contact;
use contact::{SideGate, gate_admits, segment_gate_normal};

/// The `anchor` primitive (plans/architecture-debt.md #1): entity-to-entity transform
/// slaving, extracted to one pure function so grab-hold/station-mount/ledge-hang/ship-rider
/// stop hand-rolling the same pin math. Its own file: a cross-cutting geometric primitive,
/// not a body-bus sweep concern, and it keeps this file clear of the four call sites' own
/// bookkeeping. `step::repin_ink_riders` (the only migrated caller so far) reaches it via
/// `crate::v1::body::anchor::{anchor_pin, AnchorHost}`.
pub mod anchor;

/// Ink lateral containment: the box-vs-segment SAT resolver (2026-07-06 playtest, second pass),
/// split into its own file to keep this one under the R5 file-line budget
/// (.dl/lint-file-budget.dl).
mod solid_ink;
pub use solid_ink::{apply_ink_containment, sweep_ink_containment};

/// Lip projection: the grabbable ledge points derived per frame (ledges are ink command grabs).
/// Its own file -- the ledge catch is a distinct surface read, not a Surf sweep.
pub mod lips;
pub(crate) use lips::{Lip, LipSoup, lip_world};

/// Who a surface belongs to: who to stand on, what to credit the contact to.
/// `Item`/`Fighter` are reserved, not emitted yet — an ECB is a surface that became
/// impassible in Ultimate, and items become terrain when their row says so.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SurfOwner {
    Platform(u8),
    Ink(u8),
    Item(u8),
    Fighter(u8),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SurfKind {
    /// Walkable: endpoints ordered `a.x <= b.x` at emission.
    Floor,
    /// Blocking: endpoints ordered `a.y <= b.y` at emission.
    Wall,
}

/// One collision segment in the soup, whoever owns it.
#[derive(Clone, Copy, Debug)]
pub struct Surf {
    pub a: Vector2,
    pub b: Vector2,
    /// false = drop-through with held down (soft platform, soft ink). Floors only;
    /// walls block regardless (a steep run of a soft stroke still walls you).
    pub solid: bool,
    /// Surface velocity (px/s): a rider standing on it inherits this at landing (the
    /// Randall hook, `sweep_floors`' `FloorHit.vel` -> `n.vel += hit.vel` at every landing
    /// site in za_warudo.rs). NOT an independent number from the continuous-carry seam below
    /// (`path_surface_vel`) -- an Ink Floor row is emitted as `path_surface_vel(self) * FPS`
    /// (this file's `Randall for InkPath`), so the landing-frame inherit and the frame-to-frame
    /// ride/cling carry both trace to the SAME `InkPath.vel`, just read through two call sites
    /// (soup emission vs. direct `paths[slot]` index) because the ride/cling/repin sites already
    /// know their exact ink slot and a Soup re-scan to fetch the identical number would only add
    /// cost, not remove a redundancy. Platform/stage rows stay ZERO -- dormant until a moving
    /// platform exists.
    pub vel: Vector2,
    pub kind: SurfKind,
    pub owner: SurfOwner,
    /// The filter every mover's sweep checks before treating this row as a block (plans/
    /// body-unify.md step 5): `Solid`/`Soft` behave exactly as `solid` always has (this
    /// field adds nothing for them); `OneWay` can additionally admit a crossing outright.
    pub gate: SideGate,
    /// Gate reference axis, fixed at EMISSION from the surface's own a->b tangent -- NEVER
    /// re-derived from `a`/`b` above, which get reordered per `kind` for the span math and would
    /// silently flip a closed stroke's winding-consistent normal segment to segment (see
    /// `stage::bake_ship`). `Solid`/`Soft` never read it.
    pub gate_normal: Vector2,
    /// Reflect restitution, lowered from `props.bounce` -- ZERO for platforms/stage walls (plans/
    /// ship-containment.md #1: "we should multi bounce in the ball").
    pub restitution: f32,
}

impl Surf {
    const EMPTY: Self = Self {
        a: Vector2::ZERO,
        b: Vector2::ZERO,
        solid: false,
        vel: Vector2::ZERO,
        kind: SurfKind::Floor,
        owner: SurfOwner::Platform(0),
        gate: SideGate::Solid,
        gate_normal: Vector2::ZERO,
        restitution: 0.0,
    };
}

/// Anything that contributes terrain. Melee precedent: Randall — a moving platform is
/// still just lines in the soup. The emitter does not know who will sweep it.
pub trait Randall {
    fn surfs(&self, owner: SurfOwner, nodes: &[InkNode], out: &mut impl FnMut(Surf));
}

/// The ONE seam every moving-surface consumer reads (plans/body-unify.md step 4): a path's
/// this-frame translation, ink-native units (px/frame) — whatever produced it. A KINEMATIC
/// baked fixture (mass 0, the mover) has it written from `SimState.tick` by `drive_mover`,
/// BEFORE the frame's ink snapshot; a live TRAVELING stroke (mass > 0) has it written by
/// `integrate_ink`'s gravity/bounce solve or `resolve_ink_billiard`'s writeback. Either way
/// `vel` already holds this frame's answer by the time `Soup::collect` runs, so this is a
/// pure projection, never a new source of truth — the consumer never asks WHY the surface
/// moved, only reads the seam.
pub(crate) fn path_surface_vel(path: &InkPath) -> Vector2 {
    path.vel
}

/// The armed wall's surface velocity (px/frame) for the wall-cling ride carry; ZERO for
/// platform/stage walls or a dead slot. Same seam as the grounded ride: the cling freeze is
/// RELATIVE to the wall -- clinging to the flying hull follows its translation (both axes),
/// or the hull moves out from under an absolute-space freeze and the cling silently drops
/// (2026-07-05 playtest). A still wall's vel is ZERO, so stage/pillar clings are untouched.
pub(crate) fn cling_wall_vel(
    n: &Fighter,
    paths: &[InkPath; crate::v1::stage::MAX_DRAWN],
) -> Vector2 {
    if n.wall_ink < 0 {
        return Vector2::ZERO;
    }
    let p = &paths[n.wall_ink as usize];
    if p.active() {
        path_surface_vel(p)
    } else {
        Vector2::ZERO
    }
}

/// Whether `pos` is geometrically INSIDE the ship hull's bounding circle this frame (plans/
/// ship-containment.md §4's containment scope): read fresh off the hull's own `pos`/
/// `bound_circle` every call, never a stored per-body flag -- rollback's guarantee is geometry
/// + step order, not per-body state. Deliberately NOT `ground_ink == SHIP_SLOT` (row 3's
/// `booster_blast` gate): that only reflects last frame's LANDED surface and misses exactly the
/// case this scope exists for -- a body mid-launch, bouncing around INSIDE the bowl with
/// `ground_ink == -1` the whole time (every dome-tunneling test in ship_contain_tests.rs sets it
/// that way).
pub(crate) fn hull_contains(pos: Vector2, paths: &[InkPath; MAX_DRAWN], nodes: &[InkNode]) -> bool {
    let hull = &paths[SHIP_SLOT];
    if !hull.active() {
        return false; // no hull baked (a stripped-down test fixture) -- nothing to be inside of
    }
    let (center, radius) = hull.bound_circle(nodes);
    (pos - center).length() < radius
}

impl Randall for Platform {
    fn surfs(&self, owner: SurfOwner, _nodes: &[InkNode], out: &mut impl FnMut(Surf)) {
        let (a, b) = (
            Vector2::new(self.left, self.y),
            Vector2::new(self.right, self.y),
        );
        out(Surf {
            a,
            b,
            solid: self.solid,
            vel: Vector2::ZERO,
            kind: SurfKind::Floor,
            owner,
            gate: if self.solid {
                SideGate::Solid
            } else {
                SideGate::Soft
            }, // no one-way platforms yet
            gate_normal: segment_gate_normal(a, b), // inert until a platform ever gates
            restitution: 0.0,                       // platforms never bounce
        });
    }
}

// parity(v1-ink-surface-lowering): each live finalized path lowers its classified local segments into world floors, walls, ledges, velocity, solidity, valve orientation, and restitution without making drawing ink terrain
impl Randall for InkPath {
    fn surfs(&self, owner: SurfOwner, nodes: &[InkNode], out: &mut impl FnMut(Surf)) {
        // same gates as ink_floor_y_at / ink_wall_block: mid-draw ink is not terrain,
        // TRAVELING ink still is (you can ride a flying piece — it re-pins under you).
        if !self.active() || self.drawing {
            return;
        }
        // Surface velocity (the Randall hook, px/s): the one seam (`path_surface_vel`,
        // plans/body-unify.md step 4), converted from ink-native px/frame. ANY moving path
        // puts its vel on its Floor rows now — a KINEMATIC baked fixture (the mover, driven
        // from the tick) or a live TRAVELING stroke (still arcing/bouncing) alike — so a
        // rider inherits it at landing regardless of WHY the surface moves. A still stroke's
        // vel is ZERO, so this is a no-op for settled ink, byte-identical to before.
        let vel = path_surface_vel(self) * FPS;
        // One gate for the whole stroke, Floor and Wall sharing it, no owner special-case
        // (plans/ship-containment.md #1, 2026-07-06: SUPERSEDES 97d11f5's one-way-purple leak --
        // a solid bouncy container, exit only via the hatch gap). A baked solid fixture's
        // (`owner < 0 && props.solid`, the hull) Wall rows are `Solid` now, same as its Floor rows.
        let material = contact::material_of(&self.props);
        let gate = material.gate;
        let wall_gate = if self.owner < 0 && self.props.solid {
            SideGate::Solid
        } else {
            gate
        };
        for s in 0..(self.len as usize).saturating_sub(1) {
            let (wa, wb) = self.world_seg(s, nodes);
            // Fixed BEFORE the per-kind reorder below: the stroke's own drawing-order tangent
            // is the one consistent winding a whole closed loop shares (Surf::gate_normal doc).
            let gate_normal = segment_gate_normal(wa, wb);
            match self.seg_class(s, nodes) {
                SegClass::Floor | SegClass::Ledge => {
                    let (a, b) = if wa.x <= wb.x { (wa, wb) } else { (wb, wa) };
                    out(Surf {
                        a,
                        b,
                        solid: self.props.solid,
                        vel,
                        kind: SurfKind::Floor,
                        owner,
                        gate,
                        gate_normal,
                        restitution: material.restitution,
                    });
                }
                SegClass::Wall => {
                    let (a, b) = if wa.y <= wb.y { (wa, wb) } else { (wb, wa) };
                    out(Surf {
                        a,
                        b,
                        solid: true,
                        vel: Vector2::ZERO,
                        kind: SurfKind::Wall,
                        owner,
                        gate: wall_gate,
                        gate_normal,
                        restitution: material.restitution,
                    });
                }
                SegClass::None => {}
            }
        }
    }
}

/// Worst case: 4 platform tops + 2 stage walls + MAX_DRAWN paths x (MAX_PATH_PTS - 1) segs.
pub const MAX_SURFS: usize = 288;

/// The per-frame surface soup, collected once and swept by every falling body.
/// Fixed-cap stack buffer: no alloc inside step(). Overflow drops the excess
/// (debug-asserted; the shell-side pressure warn is the production tell).
pub struct Soup {
    buf: [Surf; MAX_SURFS],
    len: usize,
}

impl Soup {
    /// `contain`: `Some(SHIP_SLOT)` when the sweeping body is geometrically inside the hull
    /// (`hull_contains`); `None` for everyone else. Scopes which Ink paths this soup can see
    /// (plans/ship-containment.md §4 -- direction A, "a contained body sweeps only its
    /// container's surfs + platforms"): `sweep_walls`/`sweep_floors` return the FIRST matching
    /// row scanning the flat soup in array order, no owner precedence at all, so a foreign ink
    /// stroke (an ordinary settled player stroke, stage decoration) that happens to overlap the
    /// hull's bounding volume can catch a contained body before its own container's rim ever
    /// does. Platform rows and the main stage's own faces are NEVER filtered -- only Ink
    /// ownership is scoped, per the plan's "+ platforms." `None` reproduces every prior call
    /// site byte-identically (every path still gets swept).
    pub fn collect(paths: &[InkPath; MAX_DRAWN], nodes: &[InkNode], contain: Option<u8>) -> Self {
        let mut soup = Soup {
            buf: [Surf::EMPTY; MAX_SURFS],
            len: 0,
        };
        for (i, p) in PLATFORMS.iter().enumerate() {
            p.surfs(SurfOwner::Platform(i as u8), nodes, &mut |s| soup.push(s));
        }
        // the main stage's vertical faces: Platform rows carry no depth, so the solid
        // stage emits its own walls (lip to underside), same span the old branch used.
        for x in [FLOOR_LEFT, FLOOR_RIGHT] {
            let (a, b) = (Vector2::new(x, GROUND_Y), Vector2::new(x, STAGE_BOTTOM));
            soup.push(Surf {
                a,
                b,
                solid: true,
                vel: Vector2::ZERO,
                kind: SurfKind::Wall,
                owner: SurfOwner::Platform(0),
                gate: SideGate::Solid,
                gate_normal: segment_gate_normal(a, b), // inert: Solid never reads it
                restitution: 0.0,                       // the main stage's own faces never bounce
            });
        }
        for (i, p) in paths.iter().enumerate() {
            if contain.is_some_and(|only| i as u8 != only) {
                continue; // foreign ink, invisible to a contained body's sweep
            }
            p.surfs(SurfOwner::Ink(i as u8), nodes, &mut |s| soup.push(s));
        }
        soup
    }

    /// One-line call-site wrapper: derives the `contain` scope from `pos` via `hull_contains`
    /// so every za_warudo.rs/item.rs call site stays a single line. `Some(SHIP_SLOT as u8)`
    /// exactly when `pos` sits inside the hull right now, `None` otherwise.
    pub fn collect_scoped(paths: &[InkPath; MAX_DRAWN], nodes: &[InkNode], pos: Vector2) -> Self {
        let contain = hull_contains(pos, paths, nodes).then_some(SHIP_SLOT as u8);
        Self::collect(paths, nodes, contain)
    }

    pub fn surfs(&self) -> &[Surf] {
        &self.buf[..self.len]
    }

    fn push(&mut self, s: Surf) {
        debug_assert!(self.len < MAX_SURFS, "surf soup overflow");
        if self.len < MAX_SURFS {
            self.buf[self.len] = s;
            self.len += 1;
        }
    }
}

/// The 2D rigid-body view a mover projects into the impulse solver. A projection, not
/// an owner: callers copy vel/omega back to their real struct after `collide`.
/// `inv_mass` / `inv_inertia` of 0.0 encodes immovable (still ink pre-unlock, stage).
/// Unit-agnostic: both bodies must simply agree (ink pairs are px/frame).
#[derive(Clone, Copy, Debug)]
pub struct BodyBits {
    pub pos: Vector2,
    pub vel: Vector2,
    pub omega: f32,
    pub inv_mass: f32,
    pub inv_inertia: f32,
}

/// Full rigid impulse exchange at contact point `p` with unit normal `n` (a → b):
/// relative velocity includes each body's spin at the contact (ω×r), the denominator
/// carries the rotational terms, and a Coulomb-clamped tangential impulse transfers
/// spin on glancing blows. No-op when the bodies are separating. Pure and symmetric —
/// conservation tests live below.
pub fn collide(a: &mut BodyBits, b: &mut BodyBits, p: Vector2, n: Vector2, e: f32, mu: f32) {
    let ra = p - a.pos;
    let rb = p - b.pos;
    let rel = (b.vel + rb.perp() * b.omega) - (a.vel + ra.perp() * a.omega);
    let vn = rel.dot(n);
    if vn > 0.0 {
        return; // separating
    }
    let ran = ra.perp_dot(n);
    let rbn = rb.perp_dot(n);
    let denom = a.inv_mass + b.inv_mass + ran * ran * a.inv_inertia + rbn * rbn * b.inv_inertia;
    if denom <= 0.0 {
        return; // two immovables
    }
    let j = -(1.0 + e) * vn / denom;
    a.vel -= n * (j * a.inv_mass);
    b.vel += n * (j * b.inv_mass);
    a.omega -= ran * j * a.inv_inertia;
    b.omega += rbn * j * b.inv_inertia;
    // tangential friction impulse, clamped by the normal impulse (Coulomb): this is what
    // sheds a glancing blow into spin instead of letting surfaces slip freely.
    let tang = rel - n * vn;
    if tang.length_squared() > 1e-8 {
        let td = tang.normalize();
        let rat = ra.perp_dot(td);
        let rbt = rb.perp_dot(td);
        let tden = a.inv_mass + b.inv_mass + rat * rat * a.inv_inertia + rbt * rbt * b.inv_inertia;
        if tden > 0.0 {
            let jt = (-rel.dot(td) / tden).clamp(-mu * j, mu * j);
            a.vel -= td * (jt * a.inv_mass);
            b.vel += td * (jt * b.inv_mass);
            a.omega -= rat * jt * a.inv_inertia;
            b.omega += rbt * jt * b.inv_inertia;
        }
    }
}

pub struct FloorHit {
    pub y: f32,
    /// Landing x: where on the floor the body crossed. For an in-span point sample this is `pos.x`
    /// (no-op for the caller); for the swept arm's tunneling backstop it's the path-vs-segment
    /// crossing x, so the caller lands the body WHERE it crossed (not the end-of-frame x, which for
    /// a fast diagonal can sit outside the span and leave the grounded body floating off the edge).
    pub x: f32,
    /// The surface's own velocity — a landing body adds this (rider inherit).
    pub vel: Vector2,
    pub owner: SurfOwner,
}

/// The crossed-from-above sweep, one home (previously written per-surface-world in
/// integrate_collide x2, the specials branch x2, hitstun_slide, integrate_ink, and
/// update_items). Among Floor surfs spanning `pos.x` whose height the body crossed this
/// frame (prev at-or-above with 1px slop, now at-or-below), the HIGHEST wins — the
/// first surface a falling body meets. `drop_soft` skips non-solid rows (held down).
///
/// 2026-07-07 playtest ("slip thru blue for no reason, wet paper"): the old point sample
/// at `pos.x` against each floor's x-span tunneled on a fast DIAGONAL descent — the
/// fighter's path plainly crossed the floor's y-line mid-frame at an x that ended up
/// OUTSIDE the span by frame's end, so the floor was skipped and the fighter sailed
/// through. The SWEPT ARM below closes that: when `pos.x` is outside the span, cast the
/// fighter's PATH (prev..pos) against the floor segment via `closest_seg_seg`; if they
/// cross, the closest pair coincides and the crossing y is the landing height. The point
/// sample stays the fast path (no cast) when `pos.x` is in span; the swept arm only fires
/// on the tunneling shape.
// parity(v1-ink-floor-admission): shallow and top arc facets remain landable floors, soft ink is skipped only while dropping, and each curved segment applies its own local valve normal to the downward sweep
pub fn sweep_floors(
    prev: Vector2,
    pos: Vector2,
    drop_soft: bool,
    surfs: &[Surf],
) -> Option<FloorHit> {
    let mut best: Option<FloorHit> = None;
    for s in surfs {
        if s.kind != SurfKind::Floor || (drop_soft && !s.solid) {
            continue;
        }
        // one-way filter (plans/body-unify.md step 5): every floor sweep is a DOWNWARD
        // crossing by construction, so `geo::DOWN` is the mover's velocity for the gate check.
        // Deliberately `segment_gate_normal(s.a, s.b)`, NOT `s.gate_normal` (the winding-
        // consistent axis the lateral sweeps need): this LOCAL, per-segment ordering is what
        // keeps gravity landings working on every arc of a closed one-way loop (the hull) -- a
        // single consistent normal always disagrees with world-down on one hemisphere of any
        // convex loop, which would break landings there.
        if gate_admits(s.gate, segment_gate_normal(s.a, s.b), geo::DOWN) {
            continue;
        }
        // SPAN + Y-CROSSING. The point sample at pos.x is the fast path; the swept
        // `intersection_test` (path segment vs floor segment) is the universal backstop. The
        // backstop is needed for TWO shapes: (1) the fast-diagonal tunnel — pos.x ends up outside
        // the span this frame but the path plainly crossed mid-frame; (2) a SLOPED floor rising
        // along the fighter's x-travel — the point check samples y at pos.x only, so a fighter
        // crossing a rising slope (above it at prev.x, below it at pos.x) reads prev.y > y(pos.x)
        // and reports "not crossed" even though the path crossed the real segment. The path-vs-
        // segment intersection sees both.
        let in_span_now = !(pos.x < s.a.x || pos.x > s.b.x);
        let mut landing_y = 0.0;
        let mut landing_x = 0.0;
        let mut caught = false;
        if in_span_now {
            let span = s.b.x - s.a.x;
            let y = if span.abs() < 1e-3 {
                s.a.y.min(s.b.y)
            } else {
                s.a.y + (s.b.y - s.a.y) * (pos.x - s.a.x) / span
            };
            if prev.y <= y + 1.0 && pos.y >= y {
                landing_y = y;
                landing_x = pos.x;
                caught = true;
            }
        }
        if !caught {
            // swept backstop: did the path actually cross this segment's real bounded extent?
            let path_shape = (Iso::at(Vector2::ZERO), Shape::Segment { a: prev, b: pos });
            let floor_shape = (Iso::at(Vector2::ZERO), Shape::Segment { a: s.a, b: s.b });
            if NaiveGeom.intersection_test(path_shape, floor_shape) {
                let (p1, _p2) = geo::closest_seg_seg(prev, pos, s.a, s.b);
                landing_y = p1.y;
                landing_x = p1.x;
                caught = true;
            }
        }
        let (landing_y, landing_x, caught) = (landing_y, landing_x, caught);
        if caught && best.as_ref().map_or(true, |b| landing_y < b.y) {
            crate::sim_log!(
                "[floor-catch] owner={:?} y={:.0} x={:.0} prev=({:.0},{:.0}) pos=({:.0},{:.0}) span_now={} solid={}",
                s.owner,
                landing_y,
                landing_x,
                prev.x,
                prev.y,
                pos.x,
                pos.y,
                in_span_now,
                s.solid,
            );
            best = Some(FloorHit {
                y: landing_y,
                x: landing_x,
                vel: s.vel,
                owner: s.owner,
            });
        } else if pos.y - prev.y > 8.0 {
            // near-miss diagnostic on a fast descent (>8px/frame): this floor was a candidate
            // (right kind, not soft-dropped, not one-way-admitted) but neither the point sample
            // nor the swept intersection caught it. Logs the segment + reason so a tunnel repro
            // traces to the exact floor that should have caught. `reason` distinguishes the three
            // failure shapes: outside the x-span, in-span but the y-line wasn't crossed this frame,
            // or in the x-band but `intersection_test` returned false (the path didn't actually
            // cross the bounded segment).
            let span_lo = s.a.x.min(s.b.x);
            let span_hi = s.a.x.max(s.b.x);
            let y_lo = s.a.y.min(s.b.y);
            let y_hi = s.a.y.max(s.b.y);
            let near_y = pos.y.max(y_lo) - 40.0 <= y_hi && pos.y.min(y_hi) + 40.0 >= y_lo;
            if near_y {
                let path_in_x = (prev.x <= span_hi && prev.x >= span_lo)
                    || (pos.x <= span_hi && pos.x >= span_lo);
                let reason = if in_span_now {
                    "y-not-crossed"
                } else if path_in_x {
                    "swept-intersection-missed"
                } else {
                    "outside-span"
                };
                crate::sim_log!(
                    "[floor-miss] prev=({:.0},{:.0}) pos=({:.0},{:.0}) seg=({:.0},{:.0})-({:.0},{:.0}) {}",
                    prev.x,
                    prev.y,
                    pos.x,
                    pos.y,
                    s.a.x,
                    s.a.y,
                    s.b.x,
                    s.b.y,
                    reason,
                );
            }
        }
    }
    best
}

pub struct WallHit {
    pub x: f32,
    /// Outward normal x (toward the body's side): -1 = hit a wall on your right.
    pub nx: f32,
    pub owner: SurfOwner,
    pub restitution: f32, // the catching surf's own restitution -- the caller's reflect e
}

/// Swept horizontal ECB block against every Wall surf (generalized from the old
/// ink_wall_block; the stage's side faces are just two more rows now). For the wall
/// spanning the ECB's contact height, returns the corrected feet-x (leading side vert
/// pinned flush) and the outward normal. The prev_x sweep catches tunneling; the
/// overlap case shoves out toward whichever side the body came from.
///
/// 2026-07-07 playtest ("a strong hit launched me INTO the stage and I fell through the
/// bottom"): the main stage's own side faces (`Soup::collect`'s `FLOOR_LEFT`/`FLOOR_RIGHT` rows)
/// are only as tall as the ECB itself (`GROUND_Y..STAGE_BOTTOM`, 140px). The old gate tested
/// ONLY the ECB's center height (`cy = pos.y - half_h`) against the wall's own bounded y-span --
/// a body whose FEET sit just below the lip has `cy` ABOVE the wall's top, outside its span on
/// every frame, however hard it's hit, even though the box's lower half plainly overlaps the
/// wall (the ink-rim counterpart of this exact bug, fixed in 83085cd via
/// `sweep_ink_containment`'s box-vs-segment SAT resolver -- Platform-owned rows still ride this
/// fn, so it needs its own upgrade). Extends the 79ebb5f ship-hull pattern here as an OR of two
/// arms, same shape as `sweep_ink_containment`'s (`solid_ink.rs`) static-overlap-OR-swept-cast
/// resolver:
///
/// STATIC ARM (kept hand-rolled, NOT routed through `geo::Geometry::contact`): gates on the
/// ECB's full feet-to-head EXTENT against the wall's span (`ecb_top`/`ecb_bot`, replacing the
/// single `cy` sample), then samples x at the extent-clamped height (a no-op for a vertical
/// stage wall; for a sloped Wall row it picks the nearest point on the segment's own bounded
/// run). `geo::Geometry::contact` was tried first: it decomposes the Cuboid into its 4 boundary
/// edges and finds the closest POINT pair against the segment's core, which is the wrong
/// primitive here -- it hands back an unsigned separation with no memory of which side `prev_x`
/// approached from, while this fn's whole contract (`WallHit.x`/`nx`) is directional: the SAME
/// wx can resolve to `wx - half_w` (approaching from the left) or `wx + half_w` (from the
/// right), and only the `prev_x`-vs-`wx` history below (unchanged since before this fix)
/// disambiguates that. A closest-point query can't supply the entry side `contact()` never
/// tracked, so the hand-rolled extent test stays -- it is the "static" arm here for the same
/// reason `sweep_ink_containment`'s own tangent-overlap test stays hand-rolled next to its
/// `cast_shapes` arm (that file's doc, "NARROWPHASE, tangent axis").
///
/// SWEPT ARM (routed through `geo::Geometry`/`NaiveGeom::cast_shapes`, the seam 79ebb5f wired):
/// backstops the static extent test for the one case a same-frame position sample can't see --
/// a body whose CURRENT extent has already carried past the wall's span this frame (fast enough
/// that the STATIC test above reads "clear," same shape as the fast-diagonal ship-hull clip
/// that fn closed) but whose swept path from `prev_x` to `pos.x` (at the current height, this
/// fn's only known y -- `sweep_walls` is never given `prev_y`) genuinely crossed the segment's
/// real bounded extent along the way. Cast the ECB `Cuboid` from `(prev_x, cy)` over that
/// horizontal displacement against the wall's own `Shape::Segment`; a hit widens the gate the
/// same way the static extent overlap does, so every candidate/one-way/shove-out branch below
/// -- unchanged -- runs off the SAME `wx` either arm computes.
pub fn sweep_walls(
    prev_x: f32,
    pos: Vector2,
    half_w: f32,
    half_h: f32,
    surfs: &[Surf],
) -> Option<WallHit> {
    let cy = pos.y - half_h; // ECB center height
    let ecb_top = pos.y - 2.0 * half_h; // head -- the box's top edge
    let ecb_bot = pos.y; // feet -- the box's bottom edge
    for s in surfs {
        if s.kind != SurfKind::Wall {
            continue;
        }
        // PRECONDITION, both arms: a body's feet must have genuinely SUNK below ground level --
        // strictly, `ecb_bot > s.a.y`, not `>=` -- before either arm below ever engages. A body
        // standing flush on the main floor has `ecb_bot == GROUND_Y == s.a.y` EXACTLY every
        // ordinary frame (that's what "resting on the floor" means), same for a WALKING grounded
        // body approaching a stage edge; `>=` would let either arm fire for them (the static test
        // trivially, the swept cast at the shared corner point where the box's flush bottom edge
        // touches the wall's own top endpoint), clamping a grounded fighter at `wx -/+ half_w`
        // well short of the platform's own edge-clamp (the real mechanism that already bounds
        // grounded walking, za_warudo.rs) -- a real behavior change this fn must not introduce.
        // Only the below-the-lip launch this fn exists for -- feet strictly below `GROUND_Y` --
        // ever reaches the extent/swept tests at all.
        if ecb_bot <= s.a.y {
            continue;
        }
        // STATIC ARM: full extent overlap, not just the center sample -- `s.a.y <= s.b.y` by the
        // Wall-kind emission convention (Surf::kind's doc), so this is a plain interval-overlap test.
        let extent_overlaps = ecb_top <= s.b.y;
        // SWEPT ARM: does the box's horizontal motion this frame (prev_x -> pos.x, held at the
        // current center height `cy` -- this fn is never given `prev_y`) cross the wall's real
        // bounded segment anywhere along the way, even though the static test above says the box's
        // final extent already cleared it? Mirrors `sweep_ink_containment`'s `swept_touch` arm
        // one-for-one (this fn's doc, "SWEPT ARM").
        let swept_touches = !extent_overlaps && {
            let mover = (
                Iso::at(Vector2::new(prev_x, cy)),
                Shape::Cuboid {
                    half: Vector2::new(half_w, half_h),
                },
            );
            let wall = (Iso::at(Vector2::ZERO), Shape::Segment { a: s.a, b: s.b });
            NaiveGeom
                .cast_shapes(
                    mover,
                    Vector2::new(pos.x - prev_x, 0.0),
                    wall,
                    Vector2::ZERO,
                    1.0,
                )
                .is_some()
        };
        if !extent_overlaps && !swept_touches {
            continue;
        }
        let span = s.b.y - s.a.y;
        // Wall kind guarantees dy dominates, so x(y) along the segment is single-valued. Clamp
        // the sample height into the wall's own span: `cy` can sit outside it (the whole point
        // of the extent gate above) while the box still genuinely overlaps the wall.
        let sample_y = cy.clamp(s.a.y, s.b.y);
        let wx = if span < 1e-3 {
            s.a.x.min(s.b.x)
        } else {
            s.a.x + (s.b.x - s.a.x) * (sample_y - s.a.y) / span
        };
        // Candidate block + its outward normal, found first; the one-way filter below reads
        // `nx` to derive the crossing's travel direction (`-nx`, toward the wall) before
        // deciding whether this surf actually catches it.
        let candidate = if prev_x + half_w <= wx && pos.x + half_w > wx {
            Some((wx - half_w, -1.0))
        } else if prev_x - half_w >= wx && pos.x - half_w < wx {
            Some((wx + half_w, 1.0))
        } else if pos.x - half_w < wx && pos.x + half_w > wx {
            let nx = if prev_x >= wx { 1.0 } else { -1.0 };
            Some((wx + nx * half_w, nx))
        } else {
            None
        };
        let Some((x, nx)) = candidate else {
            continue;
        };
        // one-way filter (plans/body-unify.md step 5): the travel direction crossing INTO
        // the wall is opposite its outward normal.
        let travel = Vector2::new(-nx, 0.0);
        if gate_admits(s.gate, s.gate_normal, travel) {
            continue;
        }
        crate::sim_log!(
            "[wall-collide] owner={:?} x={:.0} nx={:.0} prev_x={:.0} pos=({:.0},{:.0}) solid={}",
            s.owner,
            x,
            nx,
            prev_x,
            pos.x,
            pos.y,
            s.solid,
        );
        return Some(WallHit {
            x,
            nx,
            owner: s.owner,
            restitution: s.restitution,
        });
    }
    None
}

#[derive(Debug)]
pub struct GatedFloorHit {
    pub pos: Vector2,
    /// The gate normal at the catching segment -- caller kills `vel`'s component along it.
    pub normal: Vector2,
    pub owner: SurfOwner,
    pub restitution: f32, // the catching surf's own restitution -- the caller's reflect e
    /// Kind of the catching surf -- the WALL-vs-FLOOR policy `apply_ink_containment` reads to
    /// decide dead-stop+cling (Wall) vs restitution reflect (Floor). The user's spec (2026-07-07):
    /// "purple = cant pass THAT SIDE once u are completely not touching it" -- a Wall (purple) is
    /// a hard block both ways, ALWAYS; "the blue allows passing over" -- a Floor (blue) is never
    /// wall-like, however steep. Deciding wall-like by the resolved NORMAL axis (the old code) let
    /// a steep blue Floor's horizontal-dominant normal leak purple behavior onto a blue segment --
    /// "purple and blue together makes purple conditional relative to the blue". KIND is the spec.
    pub kind: SurfKind,
}

// The dome-shoulder / fast-diagonal / walk-speed-joint tunneling bugs this file used to fix with
// three separate point/circle-sample sweeps (`sweep_gated_floor_lateral`, `sweep_solid_ink_
// crossing`, `sweep_wall_joints`) are now ONE box-vs-segment SAT resolver,
// `solid_ink::sweep_ink_containment` (2026-07-06 playtest, second pass) -- see that file's module
// doc for the history and `apply_ink_containment` for the one-line call-site wrapper both
// za_warudo.rs collision sites use.

/// THE resource-refresh site (body-bus rule): ANY surface touch resets air resources.
/// Floors, walls, ink, and ledge grabs all route through here — wall-ladder climbing
/// off your own drawn walls is intended play, not an exploit.
pub fn touch_refresh(f: &mut Fighter, t: &Tune) {
    f.air_jumps = t.max_air_jumps as u8;
    f.air_dodges = t.max_air_dodges as u8;
}

/// Stand-on bookkeeping for a floor contact: point the fighter's ground refs at the
/// surface owner (ink landings read as grounded via `ground_plat = 0`, the existing
/// convention).
pub fn set_ground(f: &mut Fighter, owner: SurfOwner) {
    match owner {
        SurfOwner::Platform(i) => {
            f.ground_plat = i as i32;
            f.ground_ink = -1;
        }
        SurfOwner::Ink(i) => {
            f.ground_plat = 0;
            f.ground_ink = i as i8;
        }
        // reserved owners, not emitted yet: read as plain ground if they ever land here
        SurfOwner::Item(_) | SurfOwner::Fighter(_) => {
            f.ground_plat = 0;
            f.ground_ink = -1;
        }
    }
}

/// Frames an airborne wall contact stays convertible into a walljump/cling. Re-arms on every
/// fresh touch, so hugging a wall keeps the window live; spent (zeroed) by the jump itself.
/// Relocated here (was za_warudo's private `WALLJUMP_WINDOW`) so BOTH collision sweeps arm it
/// through the one `arm_wall_cling` seam below.
pub(crate) const WALL_CLING_WINDOW: u8 = 8;

/// The ONE walljump/cling arm site (body-bus: a stage face and a drawn-ink/hull wall arm
/// identically). `outward_nx` is the sign of the wall's outward horizontal normal (toward the
/// body); `owner` records the ink slot so a moving-hull cling can ride its translation
/// (`wall_ink`, `cling_wall_vel`). BOTH za_warudo collision sweeps -- `sweep_walls` and
/// `apply_ink_containment` -- call this: if only one armed, an ink/hull wall caught by the OTHER
/// sweep could bounce/dead-stop without ever becoming cling-able (the containment-path repel bug,
/// solid_ink.rs).
pub(crate) fn arm_wall_cling(n: &mut Fighter, outward_nx: f32, owner: SurfOwner) {
    n.wall_touch = WALL_CLING_WINDOW;
    n.wall_nx = outward_nx;
    n.wall_ink = if let SurfOwner::Ink(slot) = owner {
        slot as i8
    } else {
        -1
    };
}

/// True if the stick is deflected past `WALK_THRESH` AWAY from a wall whose outward normal is
/// `wall_nx` (queue-2026-07-03 item 1, buttonless walljump): fires the SAME kick a jump press
/// does, no button needed. Lives at the generic wall layer (reads only `wall_nx`, already
/// surface-agnostic between a stage face and a drawn-ink wall) so both work for free.
pub(crate) fn wall_deflect_away(wall_nx: f32, dir_sgn: f32, dir_mag: f32) -> bool {
    wall_nx != 0.0 && dir_sgn == wall_nx && dir_mag >= WALK_THRESH
}

/// True if the stick is deflected past `WALK_THRESH` INTO a wall whose outward normal is
/// `wall_nx`: the wall-cling trigger -- hold your ground on the wall instead of kicking off it.
pub(crate) fn wall_deflect_into(wall_nx: f32, dir_sgn: f32, dir_mag: f32) -> bool {
    wall_nx != 0.0 && dir_sgn == -wall_nx && dir_mag >= WALK_THRESH
}

/// px/s of horizontal approach INTO a wall past which an airborne body clings on its own, no stick
/// hold needed -- the "incident enough" trigger (2026-07-07 playtest: "if the angle is incident
/// enough it's a fucking wall cling"). ~2 px/frame @ 60Hz: comfortably above float/rest noise on a
/// dead-stopped face, well under the air-drift cap (~11 px/frame), so a deliberate drift/graze into
/// a purple wall or the ship hull grabs on while an incidental touch does not.
pub(crate) const CLING_INCIDENCE_SPEED: f32 = 120.0;

/// The wall-cling ENGAGE trigger (za_warudo's `CharState::Air` arm). Either the player is actively
/// holding INTO the wall (`wall_deflect_into`, the deliberate grab), OR the airborne approach is
/// incident enough on its own: horizontal velocity heading into the wall (`vel.x * wall_nx < 0`)
/// past `CLING_INCIDENCE_SPEED`. The velocity path is the playtest ask -- drifting into an ink/hull
/// wall grabs on instead of the old 0.4 billiard repel, without a hard stick hold. Walljump still
/// PREEMPTS this at the call site (a deflection AWAY is checked first), so grabbing on never blocks
/// kicking off.
pub(crate) fn wall_cling_incident(wall_nx: f32, vel: Vector2, dir_sgn: f32, dir_mag: f32) -> bool {
    if wall_nx == 0.0 {
        return false;
    }
    wall_deflect_into(wall_nx, dir_sgn, dir_mag)
        || (vel.x * wall_nx < 0.0 && vel.x.abs() >= CLING_INCIDENCE_SPEED)
}

/// Wall-jump payout for a jump off wall `wall_nx` (za_warudo spends the window, this writes the
/// velocity): a stick deflected past `WALK_THRESH` keeps the PM kick -- horizontal punch off the
/// wall plus the face-away turn. A NEUTRAL stick is the quiet hop: straight up along the wall,
/// facing kept, zero horizontal, so releasing to neutral before jumping out of a cling no longer
/// bounces you off in a sharp turn (2026-07-04: walljump was too touchy for the cling mechanics
/// being stacked on it).
pub(crate) fn wall_jump_apply(n: &mut Fighter, mag: f32, t: &Tune) {
    if mag >= WALK_THRESH {
        if n.wall_nx != 0.0 {
            n.facing = crate::v1::sign(n.wall_nx); // face away from the wall
        }
        n.vel.x = n.wall_nx * t.walljump_h;
    } else {
        n.vel.x = 0.0; // neutral hop: rise along the wall, keep facing
    }
    n.vel.y = t.walljump_v;
    n.fast_falling = false;
    n.cling_used = 0; // fresh airtime cling budget after any wall-jump payout
}

/// Air-state fast-fall + gravity for one frame, OR the wall-cling freeze in its place: while
/// `clinging` (queue-2026-07-03 item 1), vertical motion holds instead of falling -- the caller
/// decides `clinging` from `wall_deflect_into` plus the `Fighter::cling_used` budget vs
/// `Tune::cling_frames` and ticks that budget itself; this fn only pays out the freeze. Split out
/// of `reduce_next_state`'s `CharState::Air` arm so za_warudo.rs doesn't regrow past its ratchet.
pub(crate) fn apply_air_gravity(n: &mut Fighter, i: &InputFrame, t: &Tune, clinging: bool) {
    if clinging {
        n.vel.y = 0.0;
        return;
    }
    // fast fall (instant snap) + gravity. Gate on a STEEP-down stick: aim_y past the threshold
    // AND more vertical than horizontal, so down-forward drifting doesn't accidentally fast fall
    // (digital down alone still triggers: dir=0).
    if !n.fast_falling && n.vel.y > 0.0 && i.aim_y >= t.fastfall_threshold && i.aim_y > i.dir.abs()
    {
        n.fast_falling = true;
    }
    // AC frame: extreme gravity -- the fridge falls hard, boost is what fights it.
    let g_mult = if n.has_badge(Badge::AcCore) {
        t.ac_grav_mult
    } else {
        1.0
    };
    if n.fast_falling {
        n.vel.y = t.fastfall * g_mult.max(1.0);
    } else {
        n.vel.y += t.gravity * g_mult * DT;
        if n.vel.y > t.max_fall * g_mult {
            n.vel.y = t.max_fall * g_mult;
        }
    }
}

/// Continuity guarantee for the grounded-ink/platform pin -- the bug class behind dd91584
/// (ship-hull teleport), 677b8b5 (bowl air-walk), and every future instance: a fighter grounded
/// before AND after one step cannot have its y move by more than one continuity step (`step_max`,
/// plus a small epsilon for float slop). A branch that pins y unconditionally to "the floor at x"
/// instead of "the floor under my feet" reopens this bug; this trips on every native test run
/// (release: `debug_assert!` compiles out, zero cost).
pub(crate) fn assert_grounded_continuity(
    grounded_before: bool,
    grounded_after: bool,
    dy: f32,
    step_max: f32,
) {
    debug_assert!(
        !(grounded_before && grounded_after) || dy.abs() <= step_max + 1.0,
        "grounded-step teleport: y moved {} px in one frame (> {} + 1 continuity bound) while \
         grounded before and after",
        dy.abs(),
        step_max
    );
}

/// True when `pos` sits within `tol` px of SOME real Floor surf spanning its x, across the WHOLE
/// soup (every platform + every ink path). A fighter reported grounded (`ground_plat`/`ground_ink`
/// >= 0) must satisfy this every frame -- catches DRIFT bugs too (677b8b5: y held frozen while x
/// walked past the real surface into open air), not just single-frame teleports, since the real
/// surface and the held y diverge past `tol` within a few frames even with no single big jump.
pub(crate) fn on_real_floor(pos: Vector2, surfs: &[Surf], tol: f32) -> bool {
    surfs.iter().any(|s| {
        if s.kind != SurfKind::Floor {
            return false;
        }
        let (a, b) = if s.a.x <= s.b.x {
            (s.a, s.b)
        } else {
            (s.b, s.a)
        };
        if pos.x < a.x || pos.x > b.x {
            return false;
        }
        let span = b.x - a.x;
        let y = if span < 1e-3 {
            a.y.min(b.y)
        } else {
            a.y + (b.y - a.y) * (pos.x - a.x) / span
        };
        (pos.y - y).abs() <= tol
    })
}

#[cfg(test)]
mod tests;
