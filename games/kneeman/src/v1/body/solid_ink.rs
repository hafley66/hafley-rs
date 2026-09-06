//! Ink lateral containment: ONE box-vs-segment resolver, replacing an earlier pile of
//! point/circle-sample sweeps (2026-07-06 playtest, second pass). Split out of `body/mod.rs` to
//! stay under its file budget (`.dl/lint-file-budget.dl`).
//!
//! History (why this file looks the way it does): the first pass at the "slips through ink"
//! playtest bug added `sweep_solid_ink_crossing` (a whole-displacement swept CIRCLE, gated to
//! only fire above a body-width of per-frame displacement) and then `sweep_wall_joints` (an
//! ungated swept circle for Wall-kind rows, gated instead on a `was_ground_ink` FSM snapshot to
//! avoid false-firing near ledges). Both were patches on the same underlying mistake: testing a
//! single SAMPLE (a point, or a bare circle) against a segment instead of the fighter's actual
//! ECB BOX. A second playtest report ("still slipping through the purple inner line at the top
//! of the hull, from inside, on a slow rise near apex") exposed the gap those patches left: an
//! airborne body rising slowly has near-zero displacement (the circle sweep's size gate never
//! fires) and isn't riding `ground_ink` (the joint sweep's FSM gate never fires either) --
//! nothing caught it. Both patches are gone now, replaced by `sweep_ink_containment` below.

use super::contact::{SideGate, gate_admits};
use super::{GatedFloorHit, Surf, SurfKind, SurfOwner, touch_refresh};
use crate::v1::geo::{Geometry, Iso, NaiveGeom, Shape};
use crate::v1::{Fighter, Tune, Vector2, geo};

/// Broadphase reject, cheap box-vs-box: skip a segment the fighter's one-frame swept box
/// (`prev_pos`..`pos`, expanded by `reach`) could not possibly touch, before the SAT math below
/// ever runs. The flat `Surf` soup (`body::Soup`) has no whole-stroke `InkPath::bound_circle`
/// once every path is flattened into individual segments -- rejecting per SEGMENT is strictly
/// finer anyway for a big multi-segment stroke like the hull (~19 rim segments): at any moment
/// only the one or two nearest ever matter, a single whole-stroke circle wouldn't cull any of
/// them once the fighter is anywhere near the hull at all.
fn segment_in_reach(prev: Vector2, cur: Vector2, reach: f32, a: Vector2, b: Vector2) -> bool {
    let lo_x = prev.x.min(cur.x) - reach;
    let hi_x = prev.x.max(cur.x) + reach;
    let lo_y = prev.y.min(cur.y) - reach;
    let hi_y = prev.y.max(cur.y) + reach;
    let seg_lo_x = a.x.min(b.x);
    let seg_hi_x = a.x.max(b.x);
    let seg_lo_y = a.y.min(b.y);
    let seg_hi_y = a.y.max(b.y);
    lo_x <= seg_hi_x && hi_x >= seg_lo_x && lo_y <= seg_hi_y && hi_y >= seg_lo_y
}

/// Half-extent (the SAT projection radius) of an axis-aligned `half_w`x`half_h` box's shadow on
/// an arbitrary UNIT axis `u`: the standard box-vs-axis formula, `|u.x|*half_w + |u.y|*half_h`.
/// Exact for any axis direction, so one formula covers a near-vertical Wall's normal, a
/// near-horizontal Floor's normal, and every sloped row in between -- no per-kind branch.
fn box_radius_along(u: Vector2, half_w: f32, half_h: f32) -> f32 {
    u.x.abs() * half_w + u.y.abs() * half_h
}

/// A little clearance (px) folded into the "have I actually crossed" test: a body standing
/// exactly flush against a surface (feet/edge exactly ON the line, the ordinary resting state
/// after any of these resolves, or after the ground-follow walk's own `ink_floor_y_near` pin)
/// sits at zero penetration by construction; without slack, float rounding on that boundary can
/// flicker a hit on and off. 1px, same order as `sweep_floors`' own landing slop.
const CONTACT_SLOP: f32 = 1.0;

/// THE ink lateral-containment resolver. Is the fighter's ECB BOX -- `half_w` x `half_h`, both
/// axes, not a single sampled height and not a bare point/circle -- crossing to the far side of
/// a Solid-ink segment's line? One test, checked every frame, covers every FSM state and every
/// speed: grounded walking into a wall (or a short segment right at a Floor-to-Wall joint),
/// hitstun-launched into the hull at any angle, and an airborne body rising slowly toward the
/// hull's own overhead rim near apex all fall out of the SAME crossing condition, because none
/// of them needed a special case to begin with -- the special cases were working around the
/// point/circle sample's blind spots, not around a real difference between those situations.
///
/// BROADPHASE: `segment_in_reach` (above) rejects a segment before any of the SAT math below
/// runs -- most ink on a crowded stage is nowhere near any given fighter most frames.
///
/// NARROWPHASE, normal axis: project the box -- centered at `(pos.x, pos.y - half_h)`, the same
/// "ECB center" point the old single-height sample used, now carrying its OWN half-extents
/// instead of being sampled at that one height alone -- onto the segment's `gate_normal` axis via
/// `box_radius_along`. `entry` (the box's normal-axis position last frame) fixes which side is
/// "outside" for this crossing; the box's LEADING edge on that side (`cur*side - normal_r`)
/// going negative (past `CONTACT_SLOP`) is the block condition. This one axis is never
/// tunneling-prone on its own -- once the final position reads as past the line, it genuinely is,
/// however far the box also travelled tangentially getting there -- so it stays a single-frame
/// end-position test, and it is what gives a sustained push its "re-catch every frame" resting
/// behavior (`CONTACT_SLOP`'s doc below).
///
/// NARROWPHASE, tangent axis: EITHER of two conditions, an `overlaps_now` OR an `swept_touch`.
/// `overlaps_now` is the original single-frame test (`box_t` at `cur_center`, padded by the box's
/// own tangential radius, against the segment's bounded span) -- still needed verbatim for a body
/// that is ALREADY past the segment's exact zero-width line and continuing to press in (a
/// sustained push into a wall, or a slow walk into a joint): its relevant corner started the
/// frame already on the far side of the line, so a time-of-impact query has no "first contact" to
/// find (contact predates this frame), yet the fighter must keep reading as blocked every frame
/// (`CONTACT_SLOP`'s doc below, `wall_corner_tests::walking_at_walk_speed_into_a_short_wall_joint_
/// stays_blocked`). `swept_touch` is `geo::NaiveGeom::cast_shapes` (`impl Geometry`, backend stays
/// swappable for `ParryGeom` later), sweeping the fighter's actual ECB `Cuboid` from `prev_center`
/// to `cur_center` against the segment's real, bounded `Shape::Segment`: did the box touch the
/// ACTUAL segment (not an inflated capsule, not just its infinite line) at ANY point along the
/// sweep? This is what `overlaps_now` alone cannot see: a FAST diagonal launch can cross squarely
/// through a segment's interior mid-frame and still end up well past its endpoint (further than
/// the tangential padding) by the time the frame ends -- an end-position-only test reads that as
/// "clear of the span" and lets the fighter straight through
/// (`tests::fast_diagonal_launch_past_the_walls_far_endpoint_still_blocks`, the ship-hull-clip
/// playtest report). A swept cast has no such blind spot for a FRESH crossing: the box's own
/// corners trace real line segments across the frame, and the query asks whether any of them
/// crossed the segment's real, bounded extent, wherever that happens inside the frame. Neither
/// test alone covers both shapes of the bug; the OR of the two does, with no endpoint-disc special
/// case and no per-kind branch.
///
/// PASSABLE vs IMPASSABLE: `OneWay` reuses `gate_admits` (unchanged semantics -- admits WITH the
/// gate's oriented normal, blocks against it); `Solid` and `Soft` never admit here, matching
/// `contact::gate_admits` -- both always block a LATERAL crossing (an ordinary purple wall is
/// `Soft`-gated by the default pen preset, not `Solid`, and blocks exactly like the hull's
/// forced-`Solid` rim; `Soft`'s held-down drop-through is a separate, vertical-only mechanic
/// elsewhere, orthogonal to this test).
///
/// KIND-AWARE FENCE (ported from the retired `sweep_gated_floor_lateral`, body/mod.rs @ 91389a7):
/// Wall-kind rows run the crossing test above unconditionally. Floor-kind rows are a NARROWER
/// concern -- landing/resting on a Floor is `sweep_floors`'/the ground-follow walk's job, not
/// this one -- so a Floor row is only even a CANDIDATE here past THREE exclusions: `Soft`-gated
/// (an ordinary blue platform stroke, whose only vertical mechanic is the held-down drop-through)
/// is out outright; a row the fighter is CURRENTLY riding (`ground_ink == this row's own Ink
/// index`) is out because the ground-follow walk (`ink_floor_y_near`, integrate_collide's
/// `ground_ink >= 0` arm) already owns tracking that exact contact every frame, flush by
/// construction, and re-testing it here as a fresh "crossing" fights that pin
/// (`ship_tests::walking_the_bowl_floor_tracks_the_hull_not_the_air`); a row the fighter's
/// CURRENT x already sits under WHILE DESCENDING (`sweep_floors`' own exact x-span test, unpadded,
/// plus its "crossed from above" direction) is out even BEFORE `ground_ink` ever gets set, because
/// a body falling straight down onto a tilted section (the bowl's own near-bottom facets) can
/// cross this fn's normal-axis threshold a frame before `sweep_floors`' plain y-height test fires,
/// and reflecting that as a lateral bounce steals the landing entirely (`ship_tests::hull_is_
/// enterable_terrain`). The direction half of that exclusion matters just as much as the x-span
/// half: `sweep_floors` can never catch an ASCENDING crossing at all, so a body RISING through a
/// row it's under -- the hull's own overhead rim, approached slowly from inside -- stays a
/// candidate regardless of x-span (`ship_contain_tests::slow_rise_from_inside_never_leaks_
/// through_the_overhead_rim`, the second playtest report this whole resolver exists for). `Solid`
/// Ink floors (the hull shoulder) and `OneWay` floors (any owner) that clear all three exclusions
/// are the ones a LATERAL launch can tunnel through with no other backstop -- approaching from the
/// side, or vaulting clean past a narrow diagonal segment's x-span in one frame, the dome-shoulder
/// tunneling bug (`ship_contain_tests::inside_launch_blocked_by_the_blue_shoulder_and_the_solid_
/// equator_bounces`). Note this can't be a pure distance/clearance test instead of the riding+x-span pair
/// above (an earlier pass here tried `entry.abs() < normal_r`): a sloped Floor row's SAT
/// `normal_r` grows with how diagonal its normal is (up to `sqrt(half_w^2 + half_h^2)`, the box's
/// full corner-to-corner reach), so a generic clearance radius swallows genuine mid-air lateral
/// crossings through a steep shoulder long before the fighter ever touches it.
///
/// INK ONLY (`SurfOwner::Ink`): the main stage's own architecture (platforms, its two side
/// faces) keeps riding `sweep_walls`/`sweep_floors`, unchanged, still run first at both
/// za_warudo.rs call sites -- those aren't hand-drawn/discretized and were never the reported
/// bug. Otherwise STATE-INDEPENDENT: no FSM gate, no displacement-size gate -- this runs
/// unconditionally for every fighter, every frame, grounded or airborne alike (the Floor riding
/// guard above reads `ground_ink` as plain geometry-adjacent state, not an FSM branch). The one
/// FSM-shaped exception lives at the call site, not here: `za_warudo.rs`'s `climbed_onto_stage_
/// this_frame` skips the whole call for `LedgeRoll`/`LedgeAttack`, whose one-off `climb_onto_
/// stage` teleport makes `prev_pos` a stale, non-physical sweep origin for THIS frame (2026-07-06
/// ledge-roll regression: the teleport's `prev_pos` grazed the wall pillar's tangent-padded span
/// with no real crossing).
// parity(v1-ink-fighter-containment): descending bodies under a floor facet defer to floor landing so dome tops are not stolen by lateral repel, while walls, ascending crossings, and admitted one-way facets remain containment candidates
pub fn sweep_ink_containment(
    prev_pos: Vector2,
    pos: Vector2,
    mover_vel: Vector2,
    half_w: f32,
    half_h: f32,
    ground_ink: i8,
    surfs: &[Surf],
) -> Option<GatedFloorHit> {
    let reach = half_w.max(half_h) + (pos - prev_pos).length();
    let prev_center = Vector2::new(prev_pos.x, prev_pos.y - half_h);
    let cur_center = Vector2::new(pos.x, pos.y - half_h);
    for surf in surfs {
        let SurfOwner::Ink(ink_idx) = surf.owner else {
            continue;
        };
        // Floor-kind fence (this fn's doc, "KIND-AWARE FENCE"): an ordinary Soft-gated ink floor
        // has no lateral concern here at all, and a Floor row the fighter is CURRENTLY riding is
        // the ground-follow walk's job, not this one. A row the fighter's CURRENT x already sits
        // under (`sweep_floors`' own exact x-span test, unpadded), while DESCENDING (`mover_vel.y
        // >= 0.0`, the only direction `sweep_floors`' "crossed from above" test ever looks for),
        // is deferred too, even before `ground_ink` ever gets set: a body falling straight down
        // onto a tilted section (the bowl's own near-bottom facets) can cross this box-vs-segment
        // test's normal-axis threshold a frame before `sweep_floors`' plain y-height test fires,
        // and reflecting that as a LATERAL bounce steals the landing entirely (`ship_tests::hull_
        // is_enterable_terrain`). The `mover_vel.y >= 0.0` qualifier matters: `sweep_floors` can
        // never catch an ASCENDING crossing at all (it only tests descents), so a body RISING
        // through a row it's under -- the hull's own overhead rim, approached slowly from inside
        // -- must stay a candidate here, or it falls through the exact gap this whole resolver
        // exists to close (`ship_contain_tests::slow_rise_from_inside_never_leaks_through_the_
        // overhead_rim`). A body whose x is NOT under this row at all -- the dome-shoulder
        // tunneling case, approaching from the side or vaulting past its narrow x-span in one
        // frame -- stays a candidate regardless of direction or `ground_ink`.
        if surf.kind == SurfKind::Floor {
            // 2026-07-07 playtest ("the blue allows passing over ... purple = cant pass that side"):
            // a Floor (blue) NEVER blocks a lateral crossing, whatever its gate. Soft Floors were
            // already skipped here; Solid Floors (the SOLID_INK_SLOT, a drawn solid diagonal, the
            // hull's dome shoulder) used to ride this resolver as lateral walls -- the exact "purple
            // on blue" jank, a blue segment behaving like a purple wall. Only OneWay gates (which
            // block from one side by design) stay candidates among Floors; everything else passes.
            // Walls (purple) are unchanged: they hit this resolver from the `kind == Wall` arm above
            // and are the SOLE remaining lateral blocker, matching "purple = hard both ways".
            if !matches!(surf.gate, SideGate::OneWay { .. }) {
                continue;
            }
            let (span_lo, span_hi) = (surf.a.x.min(surf.b.x), surf.a.x.max(surf.b.x));
            let landing_under_it =
                mover_vel.y >= 0.0 && cur_center.x >= span_lo && cur_center.x <= span_hi;
            if ink_idx as i8 == ground_ink || landing_under_it {
                continue;
            }
        }
        if !segment_in_reach(prev_pos, pos, reach, surf.a, surf.b) {
            continue;
        }
        let normal = surf.gate_normal;
        let normal_r = box_radius_along(normal, half_w, half_h);

        // which side is "outside": the box's normal-axis position as of LAST frame.
        let entry = (prev_center - surf.a).dot(normal);
        let side = if entry >= 0.0 { 1.0 } else { -1.0 };
        let cur = (cur_center - surf.a).dot(normal);
        if cur * side - normal_r >= -CONTACT_SLOP {
            continue; // the box's leading edge hasn't reached the line yet from this side
        }

        // tangential span: EITHER the box's final position (padded by its own tangential
        // radius, exactly the pre-cast check) already overlaps the segment's bounded length,
        // OR the swept cast finds a genuine touch somewhere along this frame's motion (this
        // fn's doc, "NARROWPHASE, tangent axis") -- catches a straddling-and-still-pushing box
        // whose corners started the frame already past the segment's exact zero-width line (no
        // fresh crossing for a TOI cast to find, since first contact predates this frame) as well
        // as a fresh fast crossing that ends up past the span by the time the frame ends (no
        // static overlap at the end, but a real crossing happened along the way).
        let tangent = normal.perp();
        let tangent_r = box_radius_along(tangent, half_w, half_h);
        let tb = (surf.b - surf.a).dot(tangent);
        let (lo, hi) = if tb >= 0.0 { (0.0, tb) } else { (tb, 0.0) };
        let box_t = (cur_center - surf.a).dot(tangent);
        let overlaps_now = box_t + tangent_r >= lo && box_t - tangent_r <= hi;

        let mover = (
            Iso::at(prev_center),
            Shape::Cuboid {
                half: Vector2::new(half_w, half_h),
            },
        );
        let seg_shape = (
            Iso::at(Vector2::ZERO),
            Shape::Segment {
                a: surf.a,
                b: surf.b,
            },
        );
        let swept_touch = NaiveGeom
            .cast_shapes(
                mover,
                cur_center - prev_center,
                seg_shape,
                Vector2::ZERO,
                1.0,
            )
            .is_some();

        if !overlaps_now && !swept_touch {
            continue; // clear of the segment's own bounded span, at rest and along the sweep
        }

        if gate_admits(surf.gate, normal, mover_vel) {
            continue; // OneWay, and the crossing runs with its pass direction
        }

        // resolve flush to the entry side, `normal_r` of clearance: next frame's box then sits
        // exactly at the boundary, so a sustained push re-catches every frame instead of reading
        // as already-through (same reasoning the retired point-based sweeps used).
        let target = normal_r * side;
        // Store the OUTWARD normal (toward the body's entry side), not the raw winding normal: the
        // caller's dead-stop reflect is sign-invariant either way, but `apply_ink_containment`'s
        // cling arm needs the outward horizontal sign (`wall_nx` = which side the wall is on). The
        // position math above stays in the raw `normal`, unchanged.
        let out_normal = normal * side;
        return Some(GatedFloorHit {
            pos: pos + normal * (target - cur),
            normal: out_normal,
            owner: surf.owner,
            restitution: surf.restitution,
            kind: surf.kind,
        });
    }
    None
}

/// Call-site wrapper: writes back pos/vel and refreshes air resources on a blocked crossing,
/// no-op otherwise. The one line both za_warudo.rs collision sites call.
// parity(v1-ink-fighter-contact-response): ordinary fighters dead-stop on ink instead of inheriting stroke bounce, while tumbling fighters alone use restitution and remain airborne
pub fn apply_ink_containment(
    prev_pos: Vector2,
    fighter: &mut Fighter,
    tune: &Tune,
    half_w: f32,
    half_h: f32,
    surfs: &[Surf],
) {
    let hit = sweep_ink_containment(
        prev_pos,
        fighter.pos,
        fighter.vel,
        half_w,
        half_h,
        fighter.ground_ink,
        surfs,
    );
    // Debug trace: log every call where the fighter is MOVING (not idle) near Ink surfs, so a
    // tunnel or a false-block reproduces as a frame-by-frame trajectory in /tmp/smash_run.log.
    // The HIT line below shows what caught; this per-frame line shows the sweep's INPUT + that
    // it returned nothing when a crossing should have blocked. `near_ink` counts Ink-owned surfs
    // so idle-in-open-air frames stay quiet. Compiles out in release (`sim_log!` is a no-op).
    let speed = fighter.vel.length();
    if speed > 1.0 {
        let near_ink = surfs
            .iter()
            .filter(|s| matches!(s.owner, SurfOwner::Ink(_)))
            .count();
        if near_ink > 0 {
            crate::sim_log!(
                "[ink-collide] pos=({:.0},{:.0}) vel=({:.0},{:.0}) spd={:.0} state={:?} gink={}",
                fighter.pos.x,
                fighter.pos.y,
                fighter.vel.x,
                fighter.vel.y,
                speed,
                fighter.state,
                fighter.ground_ink,
            );
        }
    }
    if let Some(hit) = hit {
        crate::sim_log!(
            "[ink-collide] HIT kind={:?} owner={:?} normal=({:.2},{:.2}) pos=({:.0},{:.0}) tumble={}",
            hit.kind,
            hit.owner,
            hit.normal.x,
            hit.normal.y,
            fighter.pos.x,
            fighter.pos.y,
            fighter.tumble,
        );
        fighter.pos = hit.pos;
        // 2026-07-07 playtest ("i repel, i cannot cross it, no matter what ai i use"): a NON-tumble
        // fighter touching ANY ink surface -- Wall (purple) OR Floor (blue), however steep -- must
        // DEAD-STOP, never bounce. The old restitution reflect (PEN's 0.4 billiard row) is the repel:
        // it kicks the fighter back off the surface and overrides the cling/walljump branch right
        // below. The previous kind-based wall_like test regressed steep Solid Floors (a dome shoulder,
        // a drawn diagonal -- classified Floor, so wall_like=false, so it took the bounce branch and
        // repelled). The fix: dead-stop unconditionally for non-tumble contacts, kind-gate ONLY the
        // cling arm (a Wall is something you can hug and jump off; a Floor is not, however steep).
        // TUMBLE keeps the bounce (PM/Ultimate launched-body wall bounce + wall-tech intact).
        if !fighter.tumble {
            fighter.vel = geo::reflect(fighter.vel, hit.normal, 0.0);
            if hit.kind == SurfKind::Wall && crate::v1::state::airborne(fighter.state) {
                crate::v1::body::arm_wall_cling(fighter, crate::v1::sign(hit.normal.x), hit.owner);
            }
        } else {
            fighter.vel = geo::reflect(fighter.vel, hit.normal, hit.restitution);
        }
        touch_refresh(fighter, tune);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::body::SurfKind;

    fn wall_surf(x: f32, top: f32, bot: f32, owner: SurfOwner, gate: SideGate) -> Surf {
        let a = Vector2::new(x, top);
        let b = Vector2::new(x, bot);
        Surf {
            a,
            b,
            solid: true,
            vel: Vector2::ZERO,
            kind: SurfKind::Wall,
            owner,
            gate,
            gate_normal: crate::v1::body::contact::segment_gate_normal(a, b),
            restitution: 0.0,
        }
    }

    #[test]
    fn blocks_a_slow_approach_into_a_solid_ink_wall() {
        // The walk-speed joint repro's shape, in miniature: tiny per-frame steps, well under a
        // body-width, must still catch the crossing -- no speed gate anymore. The wall's face
        // sits at x=100; a `half_w=38` box's LEADING edge is `cur.x + half_w`, so it reaches the
        // face at cur.x=62 (minus `CONTACT_SLOP`'s 1px slack, catching at cur.x > 63) -- a fixture
        // whose `cur.x` already overlaps the face (e.g. the old 66, whose 66+38=104 is already
        // past x=100) is asserting a wrong "no hit" outcome, not a real gap in the resolver.
        let s = wall_surf(100.0, 0.0, 200.0, SurfOwner::Ink(0), SideGate::Solid);
        let hit = sweep_ink_containment(
            Vector2::new(50.0, 100.0),
            Vector2::new(51.0, 100.0), // a 1px step, leading edge (89) nowhere near the face (100)
            Vector2::new(60.0, 0.0),
            38.0,
            70.0,
            -1,
            &[s],
        );
        assert!(hit.is_none(), "still short of the wall, no hit yet");

        let hit = sweep_ink_containment(
            Vector2::new(63.0, 100.0),
            Vector2::new(64.0, 100.0), // leading edge (102) now past the face (100)
            Vector2::new(60.0, 0.0),
            38.0,
            70.0,
            -1,
            &[s],
        )
        .expect("a slow step that brings the ECB box across the wall's face must block");
        assert!(
            (hit.pos.x - 62.0).abs() < 1.0,
            "rests flush, ECB half-width off the line: {hit:?}"
        );
    }

    #[test]
    fn ordinary_soft_gated_purple_wall_blocks_same_as_solid() {
        // The default pen preset's purple wall is `Soft`, not `Solid` -- `gate_admits(Soft, ..)`
        // is always false, same as Solid, so it must block identically here. `cur.x=65` clears
        // the face (100) by more than `half_w` (38) plus `CONTACT_SLOP`'s slack, same margin
        // reasoning as the Solid case above.
        let s = wall_surf(100.0, 0.0, 200.0, SurfOwner::Ink(0), SideGate::Soft);
        let hit = sweep_ink_containment(
            Vector2::new(61.0, 100.0),
            Vector2::new(65.0, 100.0),
            Vector2::new(60.0, 0.0),
            38.0,
            70.0,
            -1,
            &[s],
        );
        assert!(
            hit.is_some(),
            "a Soft-gated ink wall blocks a lateral crossing, same as Solid"
        );
    }

    #[test]
    fn platform_owned_wall_is_never_touched() {
        // The main stage's own side faces keep riding `sweep_walls` -- this resolver is ink only.
        let s = wall_surf(100.0, 0.0, 200.0, SurfOwner::Platform(0), SideGate::Solid);
        let hit = sweep_ink_containment(
            Vector2::new(60.0, 100.0),
            Vector2::new(140.0, 100.0), // a full tunnel through, if it were eligible
            Vector2::new(4800.0, 0.0),
            38.0,
            70.0,
            -1,
            &[s],
        );
        assert!(
            hit.is_none(),
            "Platform-owned rows are never this resolver's concern"
        );
    }

    #[test]
    fn one_way_admits_the_pass_direction_and_blocks_the_other() {
        // Same face-at-x=100 margin reasoning as the Solid/Soft cases above: `cur.x=75` clears
        // the face by well more than `half_w` (38) plus `CONTACT_SLOP`, so both branches below
        // actually reach the `gate_admits` check instead of short-circuiting on "hasn't crossed
        // yet" -- the old `cur.x=63` fixture sat exactly on that earlier boundary and never got
        // far enough to test the admit logic at all.
        let normal = Vector2::new(1.0, 0.0);
        let a = Vector2::new(100.0, 0.0);
        let b = Vector2::new(100.0, 200.0);
        let passes = Surf {
            a,
            b,
            solid: true,
            vel: Vector2::ZERO,
            kind: SurfKind::Wall,
            owner: SurfOwner::Ink(0),
            gate: SideGate::OneWay { forward: true },
            gate_normal: normal,
            restitution: 0.0,
        };
        let hit = sweep_ink_containment(
            Vector2::new(61.0, 100.0),
            Vector2::new(75.0, 100.0),
            Vector2::new(60.0, 0.0),
            38.0,
            70.0,
            -1,
            &[passes],
        );
        assert!(
            hit.is_none(),
            "moving WITH the gate's oriented normal passes clean through"
        );

        let blocks = Surf {
            gate: SideGate::OneWay { forward: false },
            ..passes
        };
        let hit = sweep_ink_containment(
            Vector2::new(61.0, 100.0),
            Vector2::new(75.0, 100.0),
            Vector2::new(60.0, 0.0),
            38.0,
            70.0,
            -1,
            &[blocks],
        );
        assert!(
            hit.is_some(),
            "the same crossing against the pass direction blocks"
        );
    }

    #[test]
    fn a_body_straddling_a_short_segments_joint_still_catches_via_the_padded_tangent_overlap() {
        // Two Wall segments meeting at (100, 40): the upper one only 40px tall (under a typical
        // ECB_HALF_H of 70) -- exactly the walk-speed joint repro's shape. The box's own
        // tangential (here, vertical) radius pads the overlap test, so it still catches even
        // though the segment's OWN bounded span alone wouldn't reach the box's sampled center.
        // `cur.x=65` clears the face (100) past `half_w` (38) plus `CONTACT_SLOP`, same margin
        // reasoning as the plain-wall cases above -- the old `cur.x=63` sat exactly on that
        // earlier boundary and never reached the tangent-overlap check this test is about.
        let short_wall = wall_surf(100.0, 0.0, 40.0, SurfOwner::Ink(0), SideGate::Solid);
        let hit = sweep_ink_containment(
            Vector2::new(61.0, 40.0),
            Vector2::new(65.0, 40.0), // ECB center 70px above these feet -> well above the segment
            Vector2::new(60.0, 0.0),
            38.0,
            70.0,
            -1,
            &[short_wall],
        );
        assert!(
            hit.is_some(),
            "the joint must still catch a straddling box, short segment or not"
        );
    }

    #[test]
    fn a_body_resting_flush_on_a_floor_is_not_flagged_as_penetrating() {
        // Standing normally on a Solid ink Floor (feet exactly at the surface, box's leading
        // edge at the boundary): must NOT read as a block every frame -- that would fight the
        // ground-follow walk's own y-pin. `ground_ink=0` is the riding state that pin sets --
        // exactly the guard this test is about (the "KIND-AWARE FENCE" doc's riding guard, not
        // a distance/clearance test).
        let a = Vector2::new(0.0, 200.0);
        let b = Vector2::new(200.0, 200.0);
        let floor = Surf {
            a,
            b,
            solid: true,
            vel: Vector2::ZERO,
            kind: SurfKind::Floor,
            owner: SurfOwner::Ink(0),
            gate: SideGate::Solid,
            gate_normal: crate::v1::body::contact::segment_gate_normal(a, b),
            restitution: 0.0,
        };
        let hit = sweep_ink_containment(
            Vector2::new(100.0, 200.0),
            Vector2::new(101.0, 200.0), // walking along it, feet pinned exactly at y=200
            Vector2::new(60.0, 0.0),
            38.0,
            70.0,
            0,
            &[floor],
        );
        assert!(
            hit.is_none(),
            "resting flush on the floor is not a crossing: {hit:?}"
        );
    }

    #[test]
    fn fast_diagonal_launch_past_the_walls_far_endpoint_still_blocks() {
        // Playtest report: "I clip through the ship hull" -- a fast diagonal launch (hitstun
        // trajectory) crosses a Solid ink Wall/rim segment's line squarely through its INTERIOR
        // mid-frame (the box's leading corner passes through (100, 25), well inside this wall's
        // own 0..50 span, comfortably clear of either endpoint), but by the time the frame ENDS
        // the box's center has carried on well past the segment's bottom endpoint (50) by more
        // than the box's own half-height (70) -- past y=120. The old tangent-span check only
        // ever asked "does the box's FINAL position (padded by its own tangential radius)
        // overlap the segment's span", which reads this as `box_t - tangent_r > hi` (clear of
        // the span) and lets the fighter straight through the hull, even though the swept path
        // plainly crossed the real, bounded wall.
        let s = wall_surf(100.0, 0.0, 50.0, SurfOwner::Ink(0), SideGate::Solid);
        let hit = sweep_ink_containment(
            Vector2::new(50.0, -11.0), // prev feet: center (50, -81), well left of the face
            Vector2::new(250.0, 589.0), // cur feet: center (250, 519), deep past the face AND
            // more than a half-height past the wall's own bottom endpoint (50 + 70 = 120)
            Vector2::new(200.0, 600.0),
            38.0,
            70.0,
            -1,
            &[s],
        );
        assert!(
            hit.is_some(),
            "a fast diagonal that crosses the wall's real interior mid-frame must block, even \
             though it ends up past the segment's own endpoint by the time the frame ends"
        );
    }

    #[test]
    fn fast_diagonal_through_a_closed_two_segment_joint_still_blocks() {
        // The closed-hull variant: two Wall segments sharing a joint at (100, 50) (the hull rim
        // is exactly a chain of these), spanning y 0..100 together. Same shape as the single-
        // segment repro above, but the box's leading corner crosses squarely through the joint's
        // lower half (100, 75), inside segment #2's own 50..100 span with margin on both ends --
        // yet the frame ends with the box's center at y=569, more than a half-height (70) past
        // BOTH segments' far endpoints (100 + 70 = 170). Neither segment's old end-position-only
        // tangent check would have caught this independently; the swept cast must.
        let upper = wall_surf(100.0, 0.0, 50.0, SurfOwner::Ink(0), SideGate::Solid);
        let lower = wall_surf(100.0, 50.0, 100.0, SurfOwner::Ink(0), SideGate::Solid);
        let hit = sweep_ink_containment(
            Vector2::new(50.0, 39.0),   // prev feet: center (50, -31), left of the face
            Vector2::new(250.0, 639.0), // cur feet: center (250, 569), well past y=170
            Vector2::new(200.0, 600.0),
            38.0,
            70.0,
            -1,
            &[upper, lower],
        );
        assert!(
            hit.is_some(),
            "a fast diagonal crossing squarely through a two-segment joint's real interior must \
             block, even past both segments' padded tangential range by frame's end"
        );
    }
}
