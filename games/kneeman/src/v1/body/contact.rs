//! Contact generation + materials (plans/body-unify.md, rev 2 layering:
//! `contacts -> filters -> generic impulse solve -> overrides`).
//!
//! Step 1 gives every colliding pair ONE `Contact` dialect, produced by ShapeSet-style
//! closest-point queries over the geometry in `geo.rs` (never re-derived here). Today the
//! only live pair class is stroke-vs-stroke (the ink billiard), so `ink_vs_ink` is the one
//! producer; more pair classes emit the SAME `Contact` as they are added.
//!
//! Step 2 adds `Material` (the continuous solve inputs restitution + friction, plus the
//! geometry `SideGate`) lowered from a `StrokeProps` row. Materials carry no behavior
//! verbs -- they are numbers the generic solve reads and a gate the filters read.
//!
//! Dimension-disciplined: normals and dot products, never component special-cases.

use crate::v1::Vector2;
use crate::v1::arena::InkNode;
use crate::v1::geo;
use crate::v1::stage::{GateSide, INK_BODY_R, INK_MU, InkPath, StrokeProps};

/// One collision contact, expressed for the generic impulse solve. Every ShapeSet x
/// ShapeSet query emits this same shape (plans/body-unify.md step 1).
///
/// `normal` is unit, pointing from the first body toward the second (the `a -> b`
/// convention `collide()` expects). `depth` is penetration: positive when the two shape
/// sets overlap within their combined body radius, `<= 0` when merely within reach.
/// `relative_vel` is the second body's velocity minus the first's (informational for
/// filters / override predicates; `collide()` recomputes its own relative velocity that
/// also folds in each body's spin, so the solve does not read this field).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub point: Vector2,
    pub normal: Vector2,
    pub depth: f32,
    pub relative_vel: Vector2,
}

/// Which sides of a surface admit a contact at all -- read by the filters (geometry
/// policy) that run BEFORE the solve, not by the solve itself. Never serialized (derived
/// fresh from `StrokeProps.gate_side` at contact time): `OneWay`'s `forward` bit is free
/// to carry a payload for exactly that reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideGate {
    /// Blocks from every side (purple ink wall, stage face).
    Solid,
    /// Soft platform: contact only from the approach side; held-down drops through.
    Soft,
    /// One-way plate (plans/body-unify.md step 5, the lovers-ship bubble shield): passable
    /// crossing a gated segment WITH its oriented normal, blocked against it. `forward`
    /// picks which of a segment's two natural sides (its own drawing-order tangent,
    /// rotated 90 degrees) is the pass side -- `GateSide::PassForward`/`PassBackward` on
    /// the stroke's row flip this bit, so blue vs purple is this one field, sign-flipped.
    OneWay { forward: bool },
}

/// The continuous inputs the generic solve reads, plus the gate the filters read.
/// Lowered from a `StrokeProps` row (never serialized: derived at contact time, so no
/// SimState field and no bincode positional impact).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    pub restitution: f32,
    pub friction: f32,
    pub gate: SideGate,
}

/// Lower a stroke's tunable row into the runtime `Material` the solve + filters consume:
/// `bounce -> restitution`, `solid`/`gate_side -> gate`, and ink's Coulomb friction
/// (`INK_MU`, which `StrokeProps` has no slider for) as the tangential term. A gated row
/// (`gate_side != Off`) always lowers to `OneWay`, regardless of `solid` -- the two are
/// mutually exclusive gate choices on the same row, `gate_side` wins.
pub fn material_of(props: &StrokeProps) -> Material {
    let gate = match props.gate_side {
        GateSide::Off if props.solid => SideGate::Solid,
        GateSide::Off => SideGate::Soft,
        GateSide::PassForward => SideGate::OneWay { forward: true },
        GateSide::PassBackward => SideGate::OneWay { forward: false },
    };
    Material {
        restitution: props.bounce,
        friction: INK_MU,
        gate,
    }
}

/// The one check every physics lane shares for a `SideGate` filter (plans/body-unify.md
/// step 5): true when the gate ADMITS this crossing, so the caller drops the contact
/// entirely (no block, no solve -- passes clean through). `Solid`/`Soft` never admit here
/// -- `Soft`'s drop-through is a separate, orientation-free predicate the caller applies
/// itself (held-down input for fighters), nothing this fn decides. `OneWay` admits only
/// when the mover's velocity runs WITH the gate's oriented normal (`gate_normal`, signed
/// by `forward`); against it, the gate blocks like `Solid`. Dimension-disciplined: the
/// whole test is the sign of one dot product, never `.x`/`.y`.
// parity(v1-ink-directional-valve): contact admission uses each segment's drawing-order normal and the material's forward/backward side before any solve
pub fn gate_admits(gate: SideGate, gate_normal: Vector2, mover_vel: Vector2) -> bool {
    match gate {
        SideGate::Solid | SideGate::Soft => false,
        SideGate::OneWay { forward } => {
            let oriented = if forward { gate_normal } else { -gate_normal };
            mover_vel.dot(oriented) > 0.0
        }
    }
}

/// A segment's natural gate-normal reference axis: perpendicular to its own a->b tangent,
/// by drawing order -- the one geometric convention every consumer (fighter sweep, item
/// landing, ink billiard) derives `gate_admits`'s `gate_normal` from, so "which side is
/// forward" means the same thing everywhere a `SideGate::OneWay` is checked.
pub fn segment_gate_normal(a: Vector2, b: Vector2) -> Vector2 {
    (b - a).normalize_or_zero().perp()
}

/// The ink-billiard lane's one-way filter (plans/body-unify.md step 5): true when EITHER
/// side of the pair's gate admits this crossing, so `resolve_ink_billiard` drops the
/// contact outright, no solve. `hit.normal` points a->b; `b`'s own perspective is the
/// flipped normal + relative velocity.
pub fn ink_pair_gate_admits(a: &StrokeProps, b: &StrokeProps, hit: &Contact) -> bool {
    gate_admits(material_of(a).gate, hit.normal, hit.relative_vel)
        || gate_admits(material_of(b).gate, -hit.normal, -hit.relative_vel)
}

/// Closest contact between two ink bodies (a capsule chain each, half-thickness
/// `INK_BODY_R`): bounding-circle broadphase cull, then the minimum-distance segment pair
/// across the two chains. Returns a `Contact` when the gap is within the pair's combined
/// body radius; `None` otherwise. The narrowphase reuses `geo::closest_seg_seg`
/// (parry2d closest-points) and `geo::circles_touch` -- no geometry is re-derived here.
///
/// Behavior-preserving replacement for the old `ink_pair_contact`: same cull, same
/// segment-pair minimum, same threshold, same crossing-segment normal fallback.
pub(crate) fn ink_vs_ink(first: &InkPath, second: &InkPath, nodes: &[InkNode]) -> Option<Contact> {
    let (first_center, first_radius) = first.bound_circle(nodes);
    let (second_center, second_radius) = second.bound_circle(nodes);
    if !geo::circles_touch(
        first_center,
        first_radius + INK_BODY_R,
        second_center,
        second_radius + INK_BODY_R,
    ) {
        return None;
    }
    let mut nearest: Option<(Vector2, Vector2, f32)> = None;
    for first_segment in 0..(first.len as usize).saturating_sub(1) {
        let (first_start, first_end) = first.world_seg(first_segment, nodes);
        for second_segment in 0..(second.len as usize).saturating_sub(1) {
            let (second_start, second_end) = second.world_seg(second_segment, nodes);
            let (on_first, on_second) =
                geo::closest_seg_seg(first_start, first_end, second_start, second_end);
            let gap = (on_second - on_first).length();
            if nearest
                .as_ref()
                .map_or(true, |(_, _, best_gap)| gap < *best_gap)
            {
                nearest = Some((on_first, on_second, gap));
            }
        }
    }
    let (on_first, on_second, gap) = nearest?;
    if gap > 2.0 * INK_BODY_R {
        return None;
    }
    let normal = if gap > 1e-3 {
        (on_second - on_first) / gap
    } else {
        // exactly-crossing segments: fall back to the body center line.
        (second.pos - first.pos).normalize_or_zero()
    };
    Some(Contact {
        point: (on_first + on_second) * 0.5,
        normal,
        depth: 2.0 * INK_BODY_R - gap,
        relative_vel: second.vel - first.vel,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::v1::arena::NODE_POOL;

    /// A finalized horizontal bar body written into the shared pool at `start`: two local nodes
    /// about `pos`. Returns the head; the geometry lives in `nodes`.
    fn bar(center: Vector2, half_len: f32, start: u16, nodes: &mut [InkNode]) -> InkPath {
        let mut path = InkPath::EMPTY;
        path.pos = center;
        path.start = start;
        path.len = 2;
        path.mass = 1.0;
        nodes[start as usize].pt = Vector2::new(-half_len, 0.0);
        nodes[start as usize + 1].pt = Vector2::new(half_len, 0.0);
        path
    }

    #[test]
    fn material_lowers_bounce_and_solid() {
        let soft = material_of(&StrokeProps::PEN);
        assert_eq!(soft.restitution, StrokeProps::PEN.bounce);
        assert_eq!(soft.friction, INK_MU);
        assert_eq!(soft.gate, SideGate::Soft);
        let wall = material_of(&StrokeProps {
            solid: true,
            bounce: 0.9,
            ..StrokeProps::PEN
        });
        assert_eq!(wall.gate, SideGate::Solid);
        assert_eq!(wall.restitution, 0.9);
    }

    #[test]
    fn material_lowers_gate_side_to_one_way_regardless_of_solid() {
        // gate_side wins over `solid` when it's set at all -- the two are mutually exclusive
        // gate choices on the same row.
        let forward = material_of(&StrokeProps {
            solid: true,
            gate_side: GateSide::PassForward,
            ..StrokeProps::PEN
        });
        assert_eq!(forward.gate, SideGate::OneWay { forward: true });
        let backward = material_of(&StrokeProps {
            gate_side: GateSide::PassBackward,
            ..StrokeProps::PEN
        });
        assert_eq!(backward.gate, SideGate::OneWay { forward: false });
    }

    #[test]
    fn gate_admits_only_fires_for_one_way_with_the_pass_direction() {
        let normal = Vector2::new(0.0, 1.0);
        let with_it = Vector2::new(0.0, 1.0);
        let against_it = Vector2::new(0.0, -1.0);
        assert!(gate_admits(
            SideGate::OneWay { forward: true },
            normal,
            with_it
        ));
        assert!(!gate_admits(
            SideGate::OneWay { forward: true },
            normal,
            against_it
        ));
        // forward=false flips the oriented normal, so the SAME travel direction now blocks.
        assert!(!gate_admits(
            SideGate::OneWay { forward: false },
            normal,
            with_it
        ));
        assert!(gate_admits(
            SideGate::OneWay { forward: false },
            normal,
            against_it
        ));
        // Solid/Soft never admit, whichever way the mover travels.
        assert!(!gate_admits(SideGate::Solid, normal, with_it));
        assert!(!gate_admits(SideGate::Soft, normal, with_it));
    }

    #[test]
    fn segment_gate_normal_is_perpendicular_to_the_tangent() {
        let n = segment_gate_normal(Vector2::new(0.0, 0.0), Vector2::new(10.0, 0.0));
        assert!(
            n.dot(Vector2::new(1.0, 0.0)).abs() < 1e-4,
            "perpendicular to a flat tangent"
        );
        assert!((n.length() - 1.0).abs() < 1e-4, "unit length");
    }

    #[test]
    fn ink_pair_gate_admits_checks_either_side_and_flips_bs_perspective() {
        let gated = StrokeProps {
            gate_side: GateSide::PassForward,
            ..StrokeProps::PEN
        };
        let solid_wall = StrokeProps {
            solid: true,
            ..StrokeProps::PEN
        };
        let hit = Contact {
            point: Vector2::ZERO,
            normal: Vector2::new(0.0, 1.0),
            depth: 1.0,
            relative_vel: Vector2::new(0.0, 5.0), // b moving +y relative to a
        };
        // a is gated PassForward, b is a plain solid wall: a's own gate admits this crossing
        // regardless of what b is, so the pair contact is dropped.
        assert!(ink_pair_gate_admits(&gated, &solid_wall, &hit));
        // neither side gated: never admits (unchanged Solid-vs-Solid behavior).
        assert!(!ink_pair_gate_admits(&solid_wall, &solid_wall, &hit));
    }

    #[test]
    fn ink_vs_ink_emits_a_contact_for_overlapping_bars() {
        // two horizontal bars stacked within a body radius: they touch.
        let mut nodes = [InkNode::ZERO; NODE_POOL];
        let lower = bar(Vector2::new(0.0, 0.0), 30.0, 0, &mut nodes);
        let upper = bar(Vector2::new(0.0, INK_BODY_R), 30.0, 2, &mut nodes);
        let hit = ink_vs_ink(&lower, &upper, &nodes).expect("bars within reach touch");
        // normal points from the lower bar toward the upper (which is at +y).
        assert!(
            hit.normal.dot(Vector2::new(0.0, 1.0)) > 0.9,
            "normal {:?}",
            hit.normal
        );
        assert!(hit.depth > 0.0, "overlapping within radius: positive depth");
    }

    #[test]
    fn ink_vs_ink_culls_far_bars() {
        let mut nodes = [InkNode::ZERO; NODE_POOL];
        let here = bar(Vector2::new(0.0, 0.0), 30.0, 0, &mut nodes);
        let far = bar(Vector2::new(0.0, 500.0), 30.0, 2, &mut nodes);
        assert!(ink_vs_ink(&here, &far, &nodes).is_none());
    }

    #[test]
    fn contact_relative_vel_is_second_minus_first() {
        let mut nodes = [InkNode::ZERO; NODE_POOL];
        let mut lower = bar(Vector2::new(0.0, 0.0), 30.0, 0, &mut nodes);
        let mut upper = bar(Vector2::new(0.0, INK_BODY_R), 30.0, 2, &mut nodes);
        lower.vel = Vector2::new(1.0, 0.0);
        upper.vel = Vector2::new(4.0, 0.0);
        let hit = ink_vs_ink(&lower, &upper, &nodes).unwrap();
        assert_eq!(hit.relative_vel, Vector2::new(3.0, 0.0));
    }
}
