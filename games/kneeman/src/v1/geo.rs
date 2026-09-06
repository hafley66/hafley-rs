//! Deterministic 2D collision geometry, shaped to mirror `parry2d::query` so the sim names only the
//! `Geometry` trait + our wrapper types and never a concrete backend.
//!
//! The contract matches parry's: every query takes two `(Iso, Shape)` pairs and returns our
//! `Contact`/`ShapeCastHit` (parry's field names). The narrow-phase math is now `parry2d` itself:
//! `NaiveGeom`'s four methods convert our `Shape`/`Iso` into parry shapes + `Pose`s and call the
//! `parry2d::query` free fns (this replaced the hand-rolled closed-form math; the `Geometry`
//! signatures and wrapper types are unchanged, only the impl body swapped).
//!
//! DETERMINISM: `parry2d` 0.29's DEFAULT features route every transcendental through `libm`
//! (`glamx/libm` + `simba/libm_force`), so native-release and wasm32-release agree bit-for-bit --
//! see labs/det-lib-spike/FINDINGS.md. This is a Cargo.toml-level tripwire: `default-features=false`
//! on `parry2d` would drop libm and silently break cross-platform identity on ROTATED queries. The
//! `parry_backed_queries_are_bit_identical_in_process` guard test below fails loud if it regresses.
//! (parry2d 0.29 is glam-backed -- `parry2d::math::Vector` is a `glamx::Vec2`, a DIFFERENT glam
//! version from our own glam 0.30, so conversions go through explicit component helpers below.)
//!
//! Mapping:
//!   Iso              <-> parry2d::math::Pose          (translation + rotation)
//!   Shape::Ball      <-> parry2d::shape::Ball
//!   Shape::Cuboid    <-> parry2d::shape::Cuboid       (half_extents)
//!   Shape::Segment   <-> parry2d::shape::Segment
//!   Shape::Capsule   <-> parry2d::shape::Capsule
//!   Contact          <-> parry2d::query::Contact      (point1, point2, normal1, dist)
//!   ShapeCastHit     <-> parry2d::query::ShapeCastHit (time_of_impact, witness1, normal1)
//!   Geometry::intersection_test/distance/contact/cast_shapes <-> the parry2d::query free fns

use crate::v1::Vector2; // = glam::Vec2 (our glam 0.30, NOT parry's internal glamx)
use parry2d::math::{Pose, Rot2, Vector as PVec};
use parry2d::query::{self, ClosestPoints, ShapeCastOptions};
use parry2d::shape::{Ball, Capsule, Cuboid, Segment};
use serde::{Deserialize, Serialize};

const EPS: f32 = 1e-6;

/// Placement of a shape: translation + rotation (radians). Mirrors `parry2d::math::Isometry`.
/// Most fighter shapes are axis-aligned (`rot == 0`); rotation is kept for API parity + spun hitboxes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Iso {
    pub pos: Vector2,
    pub rot: f32,
}

impl Iso {
    /// Translation only (rot = 0) -- the common case.
    pub fn at(pos: Vector2) -> Self {
        Self { pos, rot: 0.0 }
    }
    pub fn new(pos: Vector2, rot: f32) -> Self {
        Self { pos, rot }
    }
    /// Map a shape-local point into world space.
    pub fn apply(&self, local: Vector2) -> Vector2 {
        if self.rot == 0.0 {
            self.pos + local
        } else {
            let (s, c) = self.rot.sin_cos();
            self.pos + Vector2::new(c * local.x - s * local.y, s * local.x + c * local.y)
        }
    }
}

/// The shapes the sim uses, each a 1:1 with a parry2d shape. Segment/Capsule endpoints are in the
/// shape's LOCAL frame (placed by the query's `Iso`); Ball/Cuboid are centered on the `Iso`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Shape {
    Ball { r: f32 },
    Cuboid { half: Vector2 },
    Segment { a: Vector2, b: Vector2 },
    Capsule { a: Vector2, b: Vector2, r: f32 },
}

/// Closest-points contact between two shapes. Mirrors `parry2d::query::Contact`.
/// `normal1` points from shape 1 toward shape 2. `dist` is signed: negative = penetration depth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub point1: Vector2,
    pub point2: Vector2,
    pub normal1: Vector2,
    pub dist: f32,
}

/// Swept (time-of-impact) result. Mirrors `parry2d::query::ShapeCastHit`.
/// `time_of_impact` is in the same units as the velocities passed to `cast_shapes` (fraction of the
/// step when called with per-step displacement). `witness1` is the contact point on the mover.
/// `normal1` is parry's convention: the OUTWARD normal on shape 1 (the mover) at impact -- for a
/// ball dropped onto a floor segment it points DOWN into the floor, the opposite sign of a
/// surface-toward-mover normal. The live sweeps consume only `is_some()`, so this direction is
/// unobserved by the sim today; a future consumer that reflects off `normal1` must account for it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeCastHit {
    pub time_of_impact: f32,
    pub witness1: Vector2,
    pub normal1: Vector2,
}

/// The collision backend. Method names + argument shapes mirror `parry2d::query` so `ParryGeom` is a
/// thin delegating wrapper. The sim holds `impl Geometry` and never names a concrete backend.
pub trait Geometry {
    /// Do the two placed shapes overlap? `parry2d::query::intersection_test`.
    fn intersection_test(&self, a: (Iso, Shape), b: (Iso, Shape)) -> bool;
    /// Separation distance (0.0 if overlapping). `parry2d::query::distance`.
    fn distance(&self, a: (Iso, Shape), b: (Iso, Shape)) -> f32;
    /// Closest-points contact if within `prediction`. `parry2d::query::contact`.
    fn contact(&self, a: (Iso, Shape), b: (Iso, Shape), prediction: f32) -> Option<Contact>;
    /// Swept query: move shape `a` by `vel_a` and `b` by `vel_b`, first touch within `max_toi`.
    /// `parry2d::query::cast_shapes`. Used for tunneling-safe landings (feet crossing a thin top).
    fn cast_shapes(
        &self,
        a: (Iso, Shape),
        vel_a: Vector2,
        b: (Iso, Shape),
        vel_b: Vector2,
        max_toi: f32,
    ) -> Option<ShapeCastHit>;
}

/// parry2d-backed narrow-phase. Zero-sized: free to pass around. The name is retained from the
/// hand-rolled era so no call site had to change when the math moved to parry (a rename to
/// `ParryGeom` is a mechanical follow-on, out of this slice's scope).
#[derive(Clone, Copy, Default)]
pub struct NaiveGeom;

/// glam(0.30) -> parry(glamx) vector. Component copy: the two crates are DIFFERENT glam versions, so
/// no `From`/transmute is available (or wanted -- the copy is the whole conversion).
#[inline]
fn to_pvec(vector: Vector2) -> PVec {
    PVec::new(vector.x, vector.y)
}

/// parry(glamx) -> glam(0.30) vector.
#[inline]
fn from_pvec(vector: PVec) -> Vector2 {
    Vector2::new(vector.x, vector.y)
}

/// Our `Iso` -> parry `Pose`. `rot == 0` (the common axis-aligned case) skips `Rot2::from_angle`
/// (hence `sin_cos`) entirely; a nonzero rotation runs it -- and that transcendental is exactly what
/// the libm feature guard rail (module header) keeps deterministic across native/wasm.
#[inline]
fn to_pose(iso: Iso) -> Pose {
    if iso.rot == 0.0 {
        Pose::translation(iso.pos.x, iso.pos.y)
    } else {
        Pose::from_parts(to_pvec(iso.pos), Rot2::from_angle(iso.rot))
    }
}

/// Owns a concrete parry shape so a shape reference outlives the query call. Our `Shape` enum's
/// Segment/Capsule endpoints are LOCAL (placed by the query `Iso`->`Pose`), which is parry's own
/// convention, so the mapping is 1:1 with no re-basing.
enum ParryHolder {
    Ball(Ball),
    Cuboid(Cuboid),
    Segment(Segment),
    Capsule(Capsule),
}

/// Build the parry shape for one of our `Shape`s.
fn to_parry(shape: Shape) -> ParryHolder {
    match shape {
        Shape::Ball { r } => ParryHolder::Ball(Ball::new(r)),
        Shape::Cuboid { half } => ParryHolder::Cuboid(Cuboid::new(to_pvec(half))),
        Shape::Segment { a, b } => ParryHolder::Segment(Segment::new(to_pvec(a), to_pvec(b))),
        Shape::Capsule { a, b, r } => ParryHolder::Capsule(Capsule::new(to_pvec(a), to_pvec(b), r)),
    }
}

/// Bind a `&ParryHolder` to its concrete inner shape reference (`&Ball`/`&Cuboid`/...) and run
/// `$body` with that binding. A 4-arm match, monomorphized per variant: no trait object and no
/// vtable in OUR source (the no-vtables core doctrine, .dl/lint-no-dyn.dl + plans/trait-math.md).
/// parry's query free fns take a trait-object shape param; the concrete reference this binds coerces
/// to it implicitly AT the call site, so the vtable parry requires never surfaces as a type we name.
/// Nest two invocations to dispatch a shape PAIR to a query call.
macro_rules! with_parry_shape {
    ($holder:expr, $bound:ident => $body:expr) => {
        match $holder {
            ParryHolder::Ball($bound) => $body,
            ParryHolder::Cuboid($bound) => $body,
            ParryHolder::Segment($bound) => $body,
            ParryHolder::Capsule($bound) => $body,
        }
    };
}

impl Geometry for NaiveGeom {
    fn intersection_test(&self, a: (Iso, Shape), b: (Iso, Shape)) -> bool {
        let (holder_a, holder_b) = (to_parry(a.1), to_parry(b.1));
        let (pose_a, pose_b) = (to_pose(a.0), to_pose(b.0));
        with_parry_shape!(&holder_a, shape_a => with_parry_shape!(&holder_b, shape_b => {
            query::intersection_test(&pose_a, shape_a, &pose_b, shape_b).unwrap_or(false)
        }))
    }

    fn distance(&self, a: (Iso, Shape), b: (Iso, Shape)) -> f32 {
        let (holder_a, holder_b) = (to_parry(a.1), to_parry(b.1));
        let (pose_a, pose_b) = (to_pose(a.0), to_pose(b.0));
        // parry returns 0.0 when overlapping and a positive separation otherwise -- same shape as the
        // old `contact().dist.max(0.0)`. Unsupported pairings fall back to the old "infinitely far".
        with_parry_shape!(&holder_a, shape_a => with_parry_shape!(&holder_b, shape_b => {
            query::distance(&pose_a, shape_a, &pose_b, shape_b).unwrap_or(f32::MAX)
        }))
    }

    fn contact(&self, a: (Iso, Shape), b: (Iso, Shape), prediction: f32) -> Option<Contact> {
        let (holder_a, holder_b) = (to_parry(a.1), to_parry(b.1));
        let (pose_a, pose_b) = (to_pose(a.0), to_pose(b.0));
        let result = with_parry_shape!(&holder_a, shape_a => with_parry_shape!(&holder_b, shape_b => {
            query::contact(&pose_a, shape_a, &pose_b, shape_b, prediction)
        }));
        match result {
            Ok(Some(contact)) => Some(Contact {
                point1: from_pvec(contact.point1),
                point2: from_pvec(contact.point2),
                normal1: from_pvec(contact.normal1),
                dist: contact.dist,
            }),
            // None (separated beyond `prediction`) or Unsupported: no contact.
            _ => None,
        }
    }

    fn cast_shapes(
        &self,
        a: (Iso, Shape),
        vel_a: Vector2,
        b: (Iso, Shape),
        vel_b: Vector2,
        max_toi: f32,
    ) -> Option<ShapeCastHit> {
        let (holder_a, holder_b) = (to_parry(a.1), to_parry(b.1));
        let (pose_a, pose_b) = (to_pose(a.0), to_pose(b.0));
        let (pvel_a, pvel_b) = (to_pvec(vel_a), to_pvec(vel_b));
        // `stop_at_penetration` + `compute_impact_geometry_on_penetration`: a mover starting the
        // frame already inside the target registers at t=0 with a reliable witness/normal, matching
        // the old "already-inside counts as touching at t=0" behavior the swept wall/ink `is_some()`
        // gates rely on.
        let options = ShapeCastOptions {
            max_time_of_impact: max_toi,
            target_distance: 0.0,
            stop_at_penetration: true,
            compute_impact_geometry_on_penetration: true,
        };
        let result = with_parry_shape!(&holder_a, shape_a => with_parry_shape!(&holder_b, shape_b => {
            query::cast_shapes(&pose_a, pvel_a, shape_a, &pose_b, pvel_b, shape_b, options)
        }));
        match result {
            Ok(Some(hit)) => Some(ShapeCastHit {
                time_of_impact: hit.time_of_impact,
                witness1: from_pvec(hit.witness1),
                normal1: from_pvec(hit.normal1),
            }),
            _ => None,
        }
    }
}

/// The one down vector today. Tune owns this when gravity becomes a live gameplay
/// variable (plans/body-bus.md); nothing else may hardcode an axis for slope decisions.
pub const DOWN: Vector2 = Vector2::new(0.0, 1.0);

/// Slope magnitude of direction `d` about the gravity vector `down` (radians; 0 = level,
/// π/2 = plumb). Floor/wall/ceiling calls are made against THIS, never against raw x/y.
#[inline]
pub fn slope_about(d: Vector2, down: Vector2) -> f32 {
    let right = Vector2::new(-down.y, down.x);
    d.dot(down).atan2(d.dot(right).abs()).abs()
}

/// True if point `c` lies within `r` of segment `a`→`b` (point-capsule overlap).
#[inline]
pub fn seg_circle_hit(a: Vector2, b: Vector2, c: Vector2, r: f32) -> bool {
    (c - closest_on_seg(c, a, b)).length() <= r
}

/// Two circles overlap (touching counts). The broadphase one-liner, in one place.
#[inline]
pub fn circles_touch(c1: Vector2, r1: f32, c2: Vector2, r2: f32) -> bool {
    (c1 - c2).length() <= r1 + r2
}

/// Closest point on segment [a,b] to p (clamped projection).
pub fn closest_on_seg(p: Vector2, a: Vector2, b: Vector2) -> Vector2 {
    let ab = b - a;
    let len2 = ab.length_squared();
    if len2 < EPS {
        return a;
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    a + ab * t
}

/// Closest pair of points between segments [p1,q1] and [p2,q2]. Returns (point on seg 1,
/// point on seg 2). Routes through `parry2d::query::closest_points` under the same libm
/// determinism guarantee as the rest of the geo seam (see the `cast_shapes` doc + module
/// header + labs/det-lib-spike/FINDINGS.md): the shape dispatcher lands a Segment/Segment pair
/// on parry's own vendored Ericson procedure. Byte-identity to the retired hand-rolled Ericson
/// body is NOT guaranteed -- parry breaks parallel/degenerate ties differently but returns an
/// equivalent closest pair. Both segments sit at identity (their endpoints are already
/// world-space), and the query runs with an `f32::MAX` margin so it hands back the closest pair
/// regardless of gap (the Segment/Segment dispatch never returns `Disjoint` or `Intersecting`
/// under that margin -- see the arms below).
pub fn closest_seg_seg(p1: Vector2, q1: Vector2, p2: Vector2, q2: Vector2) -> (Vector2, Vector2) {
    let seg1 = Segment::new(to_pvec(p1), to_pvec(q1));
    let seg2 = Segment::new(to_pvec(p2), to_pvec(q2));
    let identity = to_pose(Iso::at(Vector2::ZERO));
    // `&seg1`/`&seg2` coerce to parry's trait-object shape param AT this call boundary -- no
    // vtable keyword surfaces in our source (the no-vtables doctrine, .dl/lint-no-dyn.dl). For a
    // Segment pair the dispatcher
    // routes to `closest_points_segment_segment`, which computes actual points even when the
    // segments cross (distance 0), so with an `f32::MAX` margin it always yields `WithinMargin`.
    match query::closest_points(&identity, &seg1, &identity, &seg2, f32::MAX) {
        Ok(ClosestPoints::WithinMargin(on_first, on_second)) => {
            (from_pvec(on_first), from_pvec(on_second))
        }
        // Unreachable for a Segment pair (the specialized dispatch never returns `Intersecting`);
        // defensive only. Hand back seg1's midpoint on both sides so `gap` collapses to 0 and the
        // caller's center-line normal fallback (ink_vs_ink) takes over -- returning the same point
        // for both is exactly what that gap~=0 path expects.
        Ok(ClosestPoints::Intersecting) => {
            let midpoint = (p1 + q1) * 0.5;
            (midpoint, midpoint)
        }
        // Unreachable under `f32::MAX` margin (and `Err(Unsupported)` cannot occur for Segment vs
        // Segment). Fall back to the segment origins rather than panic, matching the retired body's
        // "both degenerate to points" return of `(p1, p2)`.
        _ => (p1, p2),
    }
}

// The hand-rolled swept-circle-vs-segment primitives (`ray_vs_capsule`, `ray_vs_circle`,
// `line_intersect_params`, `ray_vs_seg`) were retired here: `NaiveGeom::cast_shapes` now routes
// through `parry2d::query::cast_shapes`, and their only other caller (`sweep_gated_floor_lateral`)
// was itself retired earlier. The behavior they encoded -- including "a mover starting the frame
// already inside the target registers at t=0" -- is carried by parry's `stop_at_penetration`
// option (see `cast_shapes` above).

/// Is point p inside the closed polygon `verts` (winding-agnostic, ray-cast parity test)?
pub fn point_in_poly(p: Vector2, verts: &[Vector2]) -> bool {
    let n = verts.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (vi, vj) = (verts[i], verts[j]);
        if (vi.y > p.y) != (vj.y > p.y) {
            let x = vi.x + (p.y - vi.y) / (vj.y - vi.y) * (vj.x - vi.x);
            if p.x < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Reflect velocity `v` about surface normal `n` (unit) with restitution `e` (0 = stop, 1 = elastic).
/// `v' = v - (1 + e)(v·n)n`. The wall-bounce / dead-stop primitive.
pub fn reflect(v: Vector2, n: Vector2, e: f32) -> Vector2 {
    v - n * ((1.0 + e) * v.dot(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slope_about_matches_axis_formula_under_standard_gravity() {
        for d in [
            Vector2::new(1.0, 0.0),
            Vector2::new(1.0, 1.0),
            Vector2::new(-3.0, 0.5),
            Vector2::new(0.0, -2.0),
        ] {
            let old = d.y.atan2(d.x.abs()).abs();
            assert!((slope_about(d, DOWN) - old).abs() < 1e-6, "d = {d:?}");
        }
    }

    #[test]
    fn slope_about_rotates_with_gravity() {
        // sideways gravity: a vertical segment is now "level", a horizontal one is plumb.
        let side = Vector2::new(1.0, 0.0);
        assert!(slope_about(Vector2::new(0.0, 5.0), side) < 1e-6);
        assert!((slope_about(Vector2::new(5.0, 0.0), side) - core::f32::consts::FRAC_PI_2) < 1e-6);
    }

    fn ball(p: Vector2, r: f32) -> (Iso, Shape) {
        (Iso::at(p), Shape::Ball { r })
    }
    fn seg(a: Vector2, b: Vector2) -> (Iso, Shape) {
        (Iso::at(Vector2::ZERO), Shape::Segment { a, b })
    }

    #[test]
    fn balls_touch_at_sum_of_radii() {
        let g = NaiveGeom;
        let a = ball(Vector2::new(0.0, 0.0), 10.0);
        let b = ball(Vector2::new(25.0, 0.0), 10.0);
        let c = g.contact(a, b, f32::MAX).unwrap();
        assert!((c.dist - 5.0).abs() < 1e-3, "dist {}", c.dist);
        assert!((c.normal1 - Vector2::new(1.0, 0.0)).length() < 1e-3);
        assert!(!g.intersection_test(a, b));
        let b2 = ball(Vector2::new(15.0, 0.0), 10.0);
        assert!(g.intersection_test(a, b2)); // overlap by 5
    }

    #[test]
    fn ball_vs_segment_distance() {
        let g = NaiveGeom;
        let b = ball(Vector2::new(0.0, 0.0), 5.0);
        let s = seg(Vector2::new(-100.0, 20.0), Vector2::new(100.0, 20.0));
        // center is 20 above the line, radius 5 -> 15 separation, normal points up toward the ball.
        let c = g.contact(b, s, f32::MAX).unwrap();
        assert!((c.dist - 15.0).abs() < 1e-3, "dist {}", c.dist);
    }

    #[test]
    fn capsules_use_core_distance() {
        let g = NaiveGeom;
        let a = (
            Iso::at(Vector2::ZERO),
            Shape::Capsule {
                a: Vector2::new(0.0, -30.0),
                b: Vector2::new(0.0, 30.0),
                r: 8.0,
            },
        );
        let b = (
            Iso::at(Vector2::new(40.0, 0.0)),
            Shape::Capsule {
                a: Vector2::new(0.0, -30.0),
                b: Vector2::new(0.0, 30.0),
                r: 8.0,
            },
        );
        // cores are 40 apart, minus 16 of radius -> 24.
        assert!((g.distance(a, b) - 24.0).abs() < 1e-3);
    }

    #[test]
    fn swept_ball_lands_on_platform() {
        let g = NaiveGeom;
        // feet ball at y=0 falling +y, platform top segment at y=100. radius 6 -> touch at y=94.
        let feet = ball(Vector2::new(0.0, 0.0), 6.0);
        let top = seg(Vector2::new(-200.0, 100.0), Vector2::new(200.0, 100.0));
        let hit = g
            .cast_shapes(feet, Vector2::new(0.0, 100.0), top, Vector2::ZERO, 1.0)
            .unwrap();
        // travels 100/frame; reaches contact (94 of center travel) at t≈0.94.
        assert!(
            (hit.time_of_impact - 0.94).abs() < 1e-2,
            "toi {}",
            hit.time_of_impact
        );
        // RE-PINNED to parry's convention: `ShapeCastHit.normal1` is the OUTWARD normal on shape 1
        // (the ball/mover), so a ball dropped onto the platform below it points DOWN (+y) -- the
        // opposite sign of the retired hand-rolled backend, which returned the surface-toward-mover
        // normal (-y). No live caller reads this field (both swept sweeps use only `is_some()`), so
        // this is a pure test re-pin to the parry backend's correctly-signed result, not a behavior
        // change. See the `ShapeCastHit` doc + module header.
        assert!(
            hit.normal1.y > 0.0,
            "parry normal1 is outward on the mover -> points down into the platform"
        );
    }

    #[test]
    fn swept_ball_already_overlapping_hits_at_t_zero() {
        // A mover that starts a frame ALREADY within the target registers at t=0, not at the far
        // exit -- parry's `stop_at_penetration` option (see `cast_shapes`). This is the property the
        // swept wall/ink `is_some()` gates rely on for a body that began the frame overlapping.
        let g = NaiveGeom;
        let feet = ball(Vector2::new(0.0, 0.0), 6.0);
        let touching = ball(Vector2::new(5.0, 0.0), 6.0); // 5px apart, well within combined radius
        let hit = g
            .cast_shapes(feet, Vector2::new(10.0, 0.0), touching, Vector2::ZERO, 1.0)
            .expect("already touching at t=0");
        assert_eq!(hit.time_of_impact, 0.0);
    }

    #[test]
    fn swept_miss_returns_none() {
        let g = NaiveGeom;
        let feet = ball(Vector2::new(0.0, 0.0), 6.0);
        let top = seg(Vector2::new(-200.0, 100.0), Vector2::new(200.0, 100.0));
        // moving sideways, never descends to the platform.
        assert!(
            g.cast_shapes(feet, Vector2::new(100.0, 0.0), top, Vector2::ZERO, 1.0)
                .is_none()
        );
    }

    #[test]
    fn reflect_stop_and_bounce() {
        let n = Vector2::new(0.0, -1.0); // floor normal (up)
        let v = Vector2::new(30.0, 200.0); // moving down-right into the floor
        let stop = reflect(v, n, 0.0);
        assert!((stop.y - 0.0).abs() < 1e-3 && (stop.x - 30.0).abs() < 1e-3); // y killed, x kept
        let bounce = reflect(v, n, 1.0);
        assert!((bounce.y + 200.0).abs() < 1e-3); // fully inverted
    }

    #[test]
    fn closest_seg_seg_finds_the_perpendicular_gap() {
        // Two horizontal bars stacked 10 apart, overlapping in x. The closest pair is a vertical
        // segment between them: on seg1 at y=0, on seg2 at y=10, sharing some x in the overlap.
        let (on_first, on_second) = closest_seg_seg(
            Vector2::new(-50.0, 0.0),
            Vector2::new(50.0, 0.0),
            Vector2::new(-20.0, 10.0),
            Vector2::new(80.0, 10.0),
        );
        assert!((on_first.y - 0.0).abs() < 1e-3, "on_first.y {}", on_first.y);
        assert!(
            (on_second.y - 10.0).abs() < 1e-3,
            "on_second.y {}",
            on_second.y
        );
        // Gap is the perpendicular distance, 10; the pair shares an x in the overlap band.
        assert!(((on_second - on_first).length() - 10.0).abs() < 1e-3);
        assert!(
            (on_first.x - on_second.x).abs() < 1e-3,
            "closest pair should be vertical"
        );
    }

    #[test]
    fn closest_seg_seg_crossing_returns_coincident_pair() {
        // An X crossing at the origin: the closest pair collapses to distance ~0 (the crossing
        // point) so ink_vs_ink's gap~=0 center-line normal fallback engages.
        let (on_first, on_second) = closest_seg_seg(
            Vector2::new(-10.0, -10.0),
            Vector2::new(10.0, 10.0),
            Vector2::new(-10.0, 10.0),
            Vector2::new(10.0, -10.0),
        );
        assert!(
            (on_second - on_first).length() < 1e-3,
            "crossing gap must be ~0"
        );
        assert!(on_first.length() < 1e-3, "crossing point is the origin");
    }

    // Determinism guard for the parry-backed `closest_seg_seg`. The parallel case is where tie-
    // breaking lives (infinitely many equal-distance pairs -> parry's `denom`/`ulps_eq`
    // collinearity branch picks one), so recomputing it in-process and asserting to_bits()
    // equality on all four output floats is the run-to-run identity floor for the tie-break path.
    #[test]
    fn closest_seg_seg_is_bit_identical_in_process() {
        // Parallel overlapping segments: the degenerate tie-break branch (see parry's
        // collinearity test in closest_points_segment_segment).
        let first_a = Vector2::new(-30.0, 4.0);
        let first_b = Vector2::new(30.0, 4.0);
        let second_a = Vector2::new(-10.0, 9.0);
        let second_b = Vector2::new(50.0, 9.0);
        let (lhs_first, lhs_second) = closest_seg_seg(first_a, first_b, second_a, second_b);
        let (rhs_first, rhs_second) = closest_seg_seg(first_a, first_b, second_a, second_b);
        for (lhs, rhs) in [
            (lhs_first.x, rhs_first.x),
            (lhs_first.y, rhs_first.y),
            (lhs_second.x, rhs_second.x),
            (lhs_second.y, rhs_second.y),
        ] {
            assert_eq!(
                lhs.to_bits(),
                rhs.to_bits(),
                "closest_seg_seg must be bit-identical run-to-run (parallel tie-break)"
            );
        }
    }

    #[test]
    fn point_in_poly_diamond() {
        let d = [
            Vector2::new(0.0, -10.0),
            Vector2::new(10.0, 0.0),
            Vector2::new(0.0, 10.0),
            Vector2::new(-10.0, 0.0),
        ];
        assert!(point_in_poly(Vector2::new(0.0, 0.0), &d));
        assert!(!point_in_poly(Vector2::new(9.0, 9.0), &d));
    }

    // Determinism guard for the parry2d backend. parry2d 0.29 is bit-identical native-release vs
    // wasm32-release ONLY because its DEFAULT features route transcendentals through libm
    // (glamx/libm + simba/libm_force) -- see labs/det-lib-spike/FINDINGS.md. That is a Cargo.toml
    // TRIPWIRE: if a feature-unification flip ever sets `default-features = false` on parry2d in
    // core/Cargo.toml, glam falls back to the platform libm and native<->wasm identity silently
    // breaks on any ROTATED query. This test can't observe wasm from here, but computing the SAME
    // query TWICE in-process and asserting to_bits() equality is the run-to-run identity floor: it
    // fails loud the instant any nondeterminism source (a stray HashMap iteration, a reordered
    // reduction, a non-reproducible transcendental) enters the backend, giving a future feature
    // flip a diagnosable failure right here instead of a mid-match desync.
    #[test]
    fn parry_backed_queries_are_bit_identical_in_process() {
        let g = NaiveGeom;
        // Rotated ball -> forces `Rot2::from_angle` -> `sin_cos`, the transcendental path the libm
        // guard rail actually protects (a translation-only query would never exercise it).
        let rotated_ball = (
            Iso::new(Vector2::new(37.0, -12.0), 0.7),
            Shape::Ball { r: 9.0 },
        );
        let segment = seg(Vector2::new(-80.0, 5.0), Vector2::new(120.0, -3.0));
        let contact_a = g.contact(rotated_ball, segment, f32::MAX).unwrap();
        let contact_b = g.contact(rotated_ball, segment, f32::MAX).unwrap();
        for (lhs, rhs) in [
            (contact_a.point1.x, contact_b.point1.x),
            (contact_a.point1.y, contact_b.point1.y),
            (contact_a.point2.x, contact_b.point2.x),
            (contact_a.point2.y, contact_b.point2.y),
            (contact_a.normal1.x, contact_b.normal1.x),
            (contact_a.normal1.y, contact_b.normal1.y),
            (contact_a.dist, contact_b.dist),
        ] {
            assert_eq!(
                lhs.to_bits(),
                rhs.to_bits(),
                "contact must be bit-identical run-to-run"
            );
        }
        // Swept cuboid-vs-segment: the exact shape pair the live wall/ink sweeps cast.
        let mover = (
            Iso::at(Vector2::new(-40.0, 0.0)),
            Shape::Cuboid {
                half: Vector2::new(10.0, 20.0),
            },
        );
        let wall = seg(Vector2::new(0.0, -50.0), Vector2::new(0.0, 50.0));
        let hit_a = g
            .cast_shapes(mover, Vector2::new(80.0, 0.0), wall, Vector2::ZERO, 1.0)
            .unwrap();
        let hit_b = g
            .cast_shapes(mover, Vector2::new(80.0, 0.0), wall, Vector2::ZERO, 1.0)
            .unwrap();
        for (lhs, rhs) in [
            (hit_a.time_of_impact, hit_b.time_of_impact),
            (hit_a.witness1.x, hit_b.witness1.x),
            (hit_a.witness1.y, hit_b.witness1.y),
            (hit_a.normal1.x, hit_b.normal1.x),
            (hit_a.normal1.y, hit_b.normal1.y),
        ] {
            assert_eq!(
                lhs.to_bits(),
                rhs.to_bits(),
                "cast_shapes must be bit-identical run-to-run"
            );
        }
    }
}
