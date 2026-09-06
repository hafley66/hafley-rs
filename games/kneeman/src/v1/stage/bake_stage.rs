//! Baked main-stage geometry as `owner < 0` `InkPath`s -- step 1a of
//! plans/stage-strokes-migration.md (the "stage IS ink" keystone). This builder produces the
//! SAME surfaces `Soup::collect` (body/mod.rs) derives from `PLATFORMS` + the `FLOOR_*` walls,
//! but expressed in the drawn-ink primitive: a polyline of classified segments, so the stage can
//! later be seeded into `SimState.paths` and collided through the one ink loop.
//!
//! BEHAVIOR-INERT for now: nothing in the live sim calls `stage_strokes` yet (only a test does).
//! Seeding into `spawn_n` + flipping the live collision reads to `paths` are deliberately deferred
//! to later steps of the plan -- doing them here would double-land fighters on both the baked
//! strokes AND the still-live `PLATFORMS` (the double-collision hazard the plan flags).
//!
//! Own file (not stage/mod.rs) because that module is at its R5 line-budget cap
//! (.dl/lint-file-budget.dl, 1570); this follows the billiard.rs / fixtures.rs extraction
//! precedent and re-exports from stage/mod.rs so call sites read `crate::v1::stage::stage_strokes`.

use crate::v1::Vector2;
use crate::v1::arena::{FreeSpans, InkNode};
use crate::v1::stage::{
    FLOOR_LEFT, FLOOR_RIGHT, GROUND_Y, InkPath, PLATFORMS, STAGE_BOTTOM, StrokeProps, bake_span,
};

/// How many baked strokes the whole stage takes. The solid main stage (PLATFORMS[0]) becomes
/// THREE strokes -- a top edge plus its two side walls -- instead of one platform, so the count is
/// the platform table plus two extra wall strokes. Every other platform stays one 2-point stroke.
pub const STAGE_STROKES: usize = PLATFORMS.len() + 2;

/// How far in from each stage lip the grabbable `Ledge` tip runs (px). The main stage top is an
/// OPEN polyline `lip -> interior -> interior -> lip`, and `classify`'s ledge pass flags the FIRST
/// and LAST `Floor` segments of an open path (the "open end" tips), leaving the middle segment as
/// plain walkable `Floor`. So this const only sets how much of the top edge reads as lip vs floor;
/// well past `min_seg` (10px) so a tip is a real segment, not a `None` sliver.
const LEDGE_TIP: f32 = 24.0;

/// One baked terrain stroke from world-space points: PEN material, density 0 (mass 0 = immovable,
/// strike/prune/zone-exempt), `owner = -1` (never expires/redraws, from `InkPath::EMPTY`),
/// `drawing = false`, `born = 0`. `pos = ZERO` so the stored offsets ARE world coords (the stage
/// never rotates or travels, so no centroid rebase is wanted -- `world_pt` stays the plain add).
/// `bake_span` writes the span and runs `classify`, exactly like `bake_pillar` / `bake_mover`.
fn terrain_stroke(
    world: &[Vector2],
    solid: bool,
    nodes: &mut [InkNode],
    free: &mut FreeSpans,
) -> InkPath {
    let mut path = InkPath::EMPTY;
    path.props = StrokeProps::PEN;
    path.props.density = 0.0; // immovable terrain, same discipline as the pillar/mover fixtures
    path.props.solid = solid; // main stage = solid (blocks, has ledges); soft platform = drop-through
    bake_span(&mut path, world, Vector2::ZERO, nodes, free);
    path
}

/// Build the whole static stage as baked `owner < 0` `InkPath`s, allocating real spans in `nodes`
/// via `free`. Reproduces `Soup::collect`'s PLATFORM + wall surfaces:
/// - `[0]` main stage TOP: open 4-node polyline `lip -> interior -> interior -> lip` at `GROUND_Y`,
///   so `classify` reads the two tips as `Ledge` (open ends) and the middle as walkable `Floor`.
/// - `[1]`, `[2]`: main stage LEFT / RIGHT wall, each a 2-node vertical stroke lip..underside,
///   classified `Wall`.
/// - `[3..]`: the soft platforms (PLATFORMS[1..]) as 2-node top segments, `solid = false`.
///
/// Why the main stage is three strokes, not one "closed-ish" polyline (as an earlier plan draft
/// imagined): `classify`'s ledge test is FORWARD-ONLY, so in any single traversal only ONE top tip
/// corners into its wall and reads `Ledge` -- the other tip's corner sits behind it and is missed.
/// A closed loop also reads the underside as a spurious `Floor`. Keeping the top its own open
/// stroke makes BOTH its ends open-end `Ledge`s, and the walls carry their own `Wall` class, which
/// is exactly the surface set the Surf soup emits today.
pub fn stage_strokes(nodes: &mut [InkNode], free: &mut FreeSpans) -> [InkPath; STAGE_STROKES] {
    let main = PLATFORMS[0]; // index 0 is always the solid main stage (ledges live on it)
    let top = main.y; // == GROUND_Y
    let (left, right) = (main.left, main.right); // == FLOOR_LEFT / FLOOR_RIGHT

    let top_edge = [
        Vector2::new(left, top),
        Vector2::new(left + LEDGE_TIP, top),
        Vector2::new(right - LEDGE_TIP, top),
        Vector2::new(right, top),
    ];
    let left_wall = [
        Vector2::new(FLOOR_LEFT, GROUND_Y),
        Vector2::new(FLOOR_LEFT, STAGE_BOTTOM),
    ];
    let right_wall = [
        Vector2::new(FLOOR_RIGHT, GROUND_Y),
        Vector2::new(FLOOR_RIGHT, STAGE_BOTTOM),
    ];

    let mut out = [InkPath::EMPTY; STAGE_STROKES];
    out[0] = terrain_stroke(&top_edge, main.solid, nodes, free);
    out[1] = terrain_stroke(&left_wall, true, nodes, free);
    out[2] = terrain_stroke(&right_wall, true, nodes, free);
    // The soft platforms (drop-through): one flat 2-point stroke each, straight from the table.
    for (idx, platform) in PLATFORMS.iter().enumerate().skip(1) {
        let seg = [
            Vector2::new(platform.left, platform.y),
            Vector2::new(platform.right, platform.y),
        ];
        out[2 + idx] = terrain_stroke(&seg, platform.solid, nodes, free);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{LEDGE_TIP, STAGE_STROKES, stage_strokes};
    use crate::v1::Vector2;
    use crate::v1::arena::{InkNode, NODE_POOL, Scratch};
    use crate::v1::body::{Randall, Soup, Surf, SurfKind, SurfOwner};
    use crate::v1::stage::{
        FLOOR_LEFT, FLOOR_RIGHT, GROUND_Y, InkPath, MAX_DRAWN, PLATFORMS, STAGE_BOTTOM, SegClass,
    };

    /// Highest walkable floor at world-x `x` among a set of Surf `Floor` rows (min y = highest on
    /// screen), linear-interpolated across the covering segment. `None` when nothing spans `x`.
    /// The one query both the baked strokes and the platform soup are compared through.
    fn floor_y(surfs: &[Surf], x: f32) -> Option<f32> {
        surfs
            .iter()
            .filter(|surf| surf.kind == SurfKind::Floor)
            .filter_map(|surf| {
                let (lo, hi) = if surf.a.x <= surf.b.x {
                    (surf.a, surf.b)
                } else {
                    (surf.b, surf.a)
                };
                if x < lo.x || x > hi.x {
                    return None;
                }
                let span = hi.x - lo.x;
                let t = if span.abs() < 1e-6 {
                    0.0
                } else {
                    (x - lo.x) / span
                };
                Some(lo.y + t * (hi.y - lo.y))
            })
            .fold(None, |best: Option<f32>, y| {
                Some(best.map_or(y, |prev| prev.min(y)))
            })
    }

    /// Every Wall row as a (a, b) endpoint tuple, deterministically ordered (`sort` on the
    /// stringified coords) so two Surf sets compare regardless of emission order.
    fn wall_endpoints(surfs: &[Surf]) -> Vec<(f32, f32, f32, f32)> {
        let mut walls: Vec<(f32, f32, f32, f32)> = surfs
            .iter()
            .filter(|surf| surf.kind == SurfKind::Wall)
            .map(|surf| (surf.a.x, surf.a.y, surf.b.x, surf.b.y))
            .collect();
        walls.sort_by(|left, right| {
            (left.0, left.1, left.2, left.3)
                .partial_cmp(&(right.0, right.1, right.2, right.3))
                .unwrap()
        });
        walls
    }

    /// Flatten a baked stage into its emitted Surf rows (the shape a falling body actually sweeps).
    fn baked_surfs(paths: &[InkPath], nodes: &[InkNode]) -> Vec<Surf> {
        let mut out = Vec::new();
        for (idx, path) in paths.iter().enumerate() {
            Randall::surfs(path, SurfOwner::Ink(idx as u8), nodes, &mut |surf| {
                out.push(surf)
            });
        }
        out
    }

    /// The main deliverable's proof: the baked strokes carry the right cached `SegClass` --
    /// a walkable `Floor` top with grabbable `Ledge` tips, and `Wall` side faces -- exactly what
    /// `classify` produces for the drawn-ink fixtures.
    #[test]
    fn stage_strokes_classify_the_stage_surfaces() {
        let mut scratch = Scratch::new();
        let baked = stage_strokes(&mut scratch.nodes, &mut scratch.free);
        assert_eq!(baked.len(), STAGE_STROKES);

        // [0] main top edge: Ledge (left tip) / Floor (interior) / Ledge (right tip).
        let top = &baked[0];
        assert_eq!(top.len, 4, "main top is a 4-node open polyline");
        assert_eq!(
            top.seg_class(0, &scratch.nodes),
            SegClass::Ledge,
            "left ledge tip"
        );
        assert_eq!(
            top.seg_class(1, &scratch.nodes),
            SegClass::Floor,
            "walkable interior"
        );
        assert_eq!(
            top.seg_class(2, &scratch.nodes),
            SegClass::Ledge,
            "right ledge tip"
        );
        // ledge tips sit exactly on the stage lips, at ground height.
        assert_eq!(
            top.world_pt(0, &scratch.nodes),
            Vector2::new(FLOOR_LEFT, GROUND_Y)
        );
        assert_eq!(
            top.world_pt(3, &scratch.nodes),
            Vector2::new(FLOOR_RIGHT, GROUND_Y)
        );
        assert!((top.world_pt(1, &scratch.nodes).x - (FLOOR_LEFT + LEDGE_TIP)).abs() < 1e-3);

        // [1],[2] side faces: Wall, spanning ground_y..stage_bottom at each lip.
        for (slot, lip_x) in [(1usize, FLOOR_LEFT), (2usize, FLOOR_RIGHT)] {
            let wall = &baked[slot];
            assert_eq!(wall.len, 2);
            assert_eq!(
                wall.seg_class(0, &scratch.nodes),
                SegClass::Wall,
                "side face is a wall"
            );
            assert_eq!(
                wall.world_pt(0, &scratch.nodes),
                Vector2::new(lip_x, GROUND_Y)
            );
            assert_eq!(
                wall.world_pt(1, &scratch.nodes),
                Vector2::new(lip_x, STAGE_BOTTOM)
            );
        }

        // [3..] soft platforms: walkable (a 2-node top classifies as an open-end Ledge, which the
        // sweep treats as Floor), one per STAGE0 soft platform, at its own y.
        for (idx, platform) in PLATFORMS.iter().enumerate().skip(1) {
            let soft = &baked[2 + idx];
            assert_eq!(soft.len, 2);
            let class = soft.seg_class(0, &scratch.nodes);
            assert!(
                matches!(class, SegClass::Floor | SegClass::Ledge),
                "soft platform {idx} is walkable, got {class:?}"
            );
            assert_eq!(
                soft.world_pt(0, &scratch.nodes),
                Vector2::new(platform.left, platform.y)
            );
            assert_eq!(
                soft.world_pt(1, &scratch.nodes),
                Vector2::new(platform.right, platform.y)
            );
        }
    }

    /// The strongest form: the baked strokes emit the SAME Surf geometry that `Soup::collect`
    /// derives from `PLATFORMS` + the main-stage walls. Compared at the Surf level -- floors by a
    /// dense highest-walkable-y grid across (and just past) the stage, walls by endpoint set.
    #[test]
    fn stage_strokes_reproduce_the_platform_soup() {
        let mut scratch = Scratch::new();
        let baked = stage_strokes(&mut scratch.nodes, &mut scratch.free);
        let baked = baked_surfs(&baked, &scratch.nodes);

        // A soup with NO drawn ink: every path EMPTY (inactive -> Randall early-returns), so the
        // only rows are the PLATFORM tops + the main stage's two wall faces.
        let no_ink = [InkPath::EMPTY; MAX_DRAWN];
        let empty_nodes = [InkNode::ZERO; NODE_POOL];
        let soup = Soup::collect(&no_ink, &empty_nodes, None);
        let platform_surfs: Vec<Surf> = soup
            .surfs()
            .iter()
            .filter(|surf| matches!(surf.owner, SurfOwner::Platform(_)))
            .copied()
            .collect();

        // Walls: the two side faces must match endpoint-for-endpoint (order-independent).
        assert_eq!(
            wall_endpoints(&baked),
            wall_endpoints(&platform_surfs),
            "baked side faces reproduce the soup's stage walls"
        );

        // Floors: same highest-walkable-y everywhere, off-stage included (both `None` there).
        // Overlap (a soft platform above the main floor) resolves identically on both sides
        // because `floor_y` takes the min across all covering rows.
        let mut x = FLOOR_LEFT - 100.0;
        while x <= FLOOR_RIGHT + 100.0 {
            let from_baked = floor_y(&baked, x);
            let from_soup = floor_y(&platform_surfs, x);
            match (from_baked, from_soup) {
                (Some(baked_y), Some(soup_y)) => assert!(
                    (baked_y - soup_y).abs() < 1e-3,
                    "floor y mismatch at x={x}: baked {baked_y} vs soup {soup_y}"
                ),
                (None, None) => {}
                (other_baked, other_soup) => {
                    panic!(
                        "floor presence mismatch at x={x}: baked {other_baked:?} vs soup {other_soup:?}"
                    )
                }
            }
            x += 25.0;
        }
    }
}
