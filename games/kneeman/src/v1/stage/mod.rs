//! All surfaces in one place: the static stage (geometry, platforms, blast zones) and the drawn
//! ink-path system (Kirby-Canvas-Curse-style). A drawn path and a stage are the SAME primitive — a
//! polyline of segments — so stage geometry and ink share this module. Everything here is pure and
//! `Copy`-friendly so it rides inside the rolled-back `SimState`. Re-exported at the crate root.

use crate::v1::arena::{FreeSpans, InkNode};
use crate::v1::body::{BodyBits, contact};
use crate::v1::combat::{Aim, Guard, PunchableFace, Swing, strike};
use crate::v1::zone::ink_blast_zone;
use crate::v1::{DT, Fighter, HITLAG_PER_DMG, Hitbox, InputFrame, SimState, Tune, Vector2, geo};
use serde::{Deserialize, Serialize};

// Battlefield-style stage: one solid main platform (with grabbable ledges) + soft platforms
// above that you land on from the top and drop through with down. All in pixel space.

/// A stage platform. `solid` = the main stage (blocks, has ledges); else a soft platform
/// (land from above, drop through with down).
#[derive(Copy, Clone)]
pub struct Platform {
    pub left: f32,
    pub right: f32,
    pub y: f32,
    pub solid: bool,
}

/// One arena's static geometry, gathered into a single `Copy`, const-constructible value: floor
/// bounds, the platform table, the blast zone, and the parked ship hull. Mirrors
/// `CharData::KNEEMAN`'s preset-row pattern (tune.rs) — `STAGE0` below is preset row 0, and every
/// scattered top-level const this module used to export (`GROUND_Y`, `PLATFORMS`, `BLAST_Y`,
/// `SHIP_R`, ...) is now a thin re-derivation of one of its fields, so no caller-facing
/// name/type/visibility changes. Threading `&StageSpec` through `step` to make stages
/// runtime-selectable is a separate later task (plans/turnkey-extension.md S1 phase B) — today
/// there is exactly one arena.
#[derive(Copy, Clone)]
pub struct StageSpec {
    // ── main platform (solid, has ledges) ──
    pub ground_y: f32,     // main platform top (resting feet-y)
    pub stage_bottom: f32, // main platform underside (matches the Stage Main ColorRect)
    pub floor_left: f32,
    pub floor_right: f32, // main platform width = floor_right - floor_left, centered on x=600
    // ── platform table: index 0 is always the solid main stage (ledges live on it), rest soft ──
    pub platforms: [Platform; 4],
    // ── blast zones: cross any edge = KO -> respawn (see `out_of_bounds`). Side/top sit well
    // outside the stage so only a launched (knocked-back) fighter reaches them; this is what makes
    // horizontal/vertical knockback actually kill (kill moves). Bottom is the classic fall-off
    // death. ──
    pub blast_y: f32,
    pub blast_top: f32,
    pub blast_left: f32,
    pub blast_right: f32,
    // ── baked test fixtures (stage/fixtures.rs): the wall pillar + the triangle-wave mover ──
    pub pillar_slot: usize,
    pub mover_slot: usize,
    // ── the ship (plans/lovers-ship.md): `SimState.paths[ship_slot]` is the parked hull ──
    pub ship_slot: usize,
    pub ship_r: f32,
    pub ship_home: Vector2,
    pub ship_segs: usize,
    pub ship_rim: [Vector2; 20], // unit `ship_segs`-gon; array size pinned at 20 (see SHIP_RIM)
}

/// The current (only) arena. Preset row 0 — the literal legacy numbers, pinned by a test in
/// `stage/tests.rs`.
// parity(v1-stage-static-surfaces): STAGE0 owns the main floor, side walls, and soft-platform coordinates
pub const STAGE0: StageSpec = {
    let ground_y = 760.0;
    let stage_bottom = 900.0;
    let floor_left = 150.0;
    let floor_right = 1050.0;
    StageSpec {
        ground_y,
        stage_bottom,
        floor_left,
        floor_right,
        platforms: [
            Platform {
                left: floor_left,
                right: floor_right,
                y: ground_y,
                solid: true,
            },
            Platform {
                left: 280.0,
                right: 540.0,
                y: 575.0,
                solid: false,
            }, // left
            Platform {
                left: 660.0,
                right: 920.0,
                y: 575.0,
                solid: false,
            }, // right
            Platform {
                left: 470.0,
                right: 730.0,
                y: 410.0,
                solid: false,
            }, // top center
        ],
        blast_y: 1600.0,   // below this = death (fall off the bottom)
        blast_top: -520.0, // above this = death (launched off the top)
        // Left kill line pushed out to -560 (was -420) to seat the bigger hull off-stage-left
        // with margin: the hull's left edge sits at ~-496, so a fighter still dies well past it.
        blast_left: -560.0,
        blast_right: 1620.0, // right of this = death
        // Reserved by occupancy, not by code: the slot is never EMPTY (len > 0 from spawn), so the
        // drawn-ink free-slot scan skips it naturally. Same deal for the two fixture slots.
        pillar_slot: MAX_DRAWN - 2,
        mover_slot: MAX_DRAWN - 3,
        ship_slot: MAX_DRAWN - 1,
        // ~1.8x the original 170px hull (plans/ac-ship-backlog.md item 3): a big hangout off-left.
        ship_r: 306.0,
        // Hull center. Off-stage left: dome top at y=214, right edge (x=116) ~34px shy of the
        // stage's left wall — a hop across, or a gimp gap to get blasted down.
        ship_home: Vector2::new(-190.0, 520.0),
        ship_segs: 20,
        // Unit 20-gon, CCW from angle 0, one vertex exactly at top and bottom. Literals because the
        // sim allows no runtime trig (cos/sin are not bit-identical across platforms; these are).
        ship_rim: [
            Vector2::new(1.0, 0.0),
            Vector2::new(0.951_056_5, 0.309_017_0),
            Vector2::new(0.809_017_0, 0.587_785_25),
            Vector2::new(0.587_785_25, 0.809_017_0),
            Vector2::new(0.309_017_0, 0.951_056_5),
            Vector2::new(0.0, 1.0),
            Vector2::new(-0.309_017_0, 0.951_056_5),
            Vector2::new(-0.587_785_25, 0.809_017_0),
            Vector2::new(-0.809_017_0, 0.587_785_25),
            Vector2::new(-0.951_056_5, 0.309_017_0),
            Vector2::new(-1.0, 0.0),
            Vector2::new(-0.951_056_5, -0.309_017_0),
            Vector2::new(-0.809_017_0, -0.587_785_25),
            Vector2::new(-0.587_785_25, -0.809_017_0),
            Vector2::new(-0.309_017_0, -0.951_056_5),
            Vector2::new(0.0, -1.0),
            Vector2::new(0.309_017_0, -0.951_056_5),
            Vector2::new(0.587_785_25, -0.809_017_0),
            Vector2::new(0.809_017_0, -0.587_785_25),
            Vector2::new(0.951_056_5, -0.309_017_0),
        ],
    }
};

pub(crate) const GROUND_Y: f32 = STAGE0.ground_y;
pub(crate) const STAGE_BOTTOM: f32 = STAGE0.stage_bottom;
pub(crate) const FLOOR_LEFT: f32 = STAGE0.floor_left;
pub(crate) const FLOOR_RIGHT: f32 = STAGE0.floor_right;

pub(crate) const LEDGE_HANG_DY: f32 = 44.0; // hang this far below the lip while holding

pub const BLAST_Y: f32 = STAGE0.blast_y;
pub const BLAST_TOP: f32 = STAGE0.blast_top;
pub const BLAST_LEFT: f32 = STAGE0.blast_left;
pub const BLAST_RIGHT: f32 = STAGE0.blast_right;

/// True when a fighter has crossed any blast zone (all four edges = a real KO surface).
// parity(v1-blast-frame): out_of_bounds reads the four STAGE0 blast edges
#[inline]
pub(crate) fn out_of_bounds(p: Vector2) -> bool {
    p.y > BLAST_Y || p.y < BLAST_TOP || p.x < BLAST_LEFT || p.x > BLAST_RIGHT
}

/// Index 0 is always the solid main stage (ledges live on it). The rest are soft platforms.
pub const PLATFORMS: [Platform; 4] = STAGE0.platforms;

/// A platform's top face as a `geo` segment (left..right at its y), in world space. The landing
/// path is still the closed-form bounding box crossing test today; this is the same surface expressed as the
/// shape the swept `NaiveGeom::cast_shapes` landing rides on, and the seam drawn-segment stages
/// (slopes, arbitrary floors) grow from — a stage becomes a list of these instead of axis rects.
pub fn platform_top(p: &Platform) -> (geo::Iso, geo::Shape) {
    (
        geo::Iso::at(Vector2::ZERO),
        geo::Shape::Segment {
            a: Vector2::new(p.left, p.y),
            b: Vector2::new(p.right, p.y),
        },
    )
}

/// The solid main stage's two vertical wall faces as `geo` segments (left, then right), from the top
/// lip down to the underside. Wall collision reflects launched bodies off these (see the wall block).
pub fn stage_walls() -> [(geo::Iso, geo::Shape); 2] {
    let z = geo::Iso::at(Vector2::ZERO);
    [
        (
            z,
            geo::Shape::Segment {
                a: Vector2::new(FLOOR_LEFT, GROUND_Y),
                b: Vector2::new(FLOOR_LEFT, STAGE_BOTTOM),
            },
        ),
        (
            z,
            geo::Shape::Segment {
                a: Vector2::new(FLOOR_RIGHT, GROUND_Y),
                b: Vector2::new(FLOOR_RIGHT, STAGE_BOTTOM),
            },
        ),
    ]
}

// ── The ship (plans/lovers-ship.md) ───────────────────────────────────────────────────────────────
// A giant circle parked off-stage-left: the hull is a BAKED INK STROKE (owner -1), so it is
// terrain + rendered + checksummed through machinery that already exists. Standing on it makes
// you crew: your c-stick becomes the helm (see the Strong-lane gate in za_warudo) and aims the
// engine. Seated at the one station (v3 simplification), attack fires the engine: the exhaust is
// a knockback volume off the rim (`booster_blast` in ship.rs) whose reaction, along `helm.aim`, is
// the hull's own thrust (step 6, "unpark the ship") — the hull carries mass (`density > 0`) and a
// zero `gravity_scale` row, so it is a real body that never falls: thrust is its only force, a
// zero-g Zelda-topdown feel, not a hover-vs-gravity balance. `owner: -1` keeps it exempt from
// prune/expiry regardless of motion; riders inherit its velocity through `Surf.vel` (body-bus) same
// as any other traveling stroke. One side effect worth flagging: `density > 0` also makes the hull
// a strikeable `PunchableFace` (mass 0 was the "not a body" sentinel gating that off) — a fighter
// can now chip/launch the hull like any other ink body. Not a goal of this step, not suppressed
// either (no special case for it), and worth a design pass later if it's unwanted.

/// `SimState.paths[SHIP_SLOT]` is the hull. Reserved by occupancy, not by code: the slot is never
/// EMPTY (len > 0 from spawn), so the drawn-ink free-slot scan skips it naturally.
pub const SHIP_SLOT: usize = STAGE0.ship_slot;
pub const SHIP_R: f32 = STAGE0.ship_r;
/// Hull center. Off-stage left: dome top at y=214, right edge (x=116) ~34px shy of the stage's
/// left wall — a hop across, or a gimp gap to get blasted down.
pub const SHIP_HOME: Vector2 = STAGE0.ship_home;
pub const SHIP_SEGS: usize = STAGE0.ship_segs;

/// Unit 20-gon, CCW from angle 0, one vertex exactly at top and bottom. Literals because the sim
/// allows no runtime trig (cos/sin are not bit-identical across platforms; these are).
pub const SHIP_RIM: [Vector2; SHIP_SEGS] = STAGE0.ship_rim;
// Station anchors + `station_anchor` (the mount seam) live in `crate::v1::station`, not here — the hull
// geometry consts they read (`SHIP_R`/`SHIP_SLOT`) are the only tie back to this module.

/// The engine, steered from the deck. `aim` is the DESIRED TRAVEL direction (unit vector — never an
/// angle, no atan2 in the sim); the flame sits on the opposite rim and blows outward along `-aim`.
/// `thrust` is this frame's throttle (0..1 from the pilot's c-stick deflection), zeroed every frame
/// nobody steers. `aim` keeps its last value so the idle engine marker stays where you left it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Helm {
    pub aim: Vector2,
    pub thrust: f32,
}

impl Helm {
    pub const PARKED: Self = Self {
        aim: Vector2::new(0.0, -1.0), // engine down, flame under the hull
        thrust: 0.0,
    };
}

/// The hull as a circle stroke with a HATCH: the two segments around the top vertex are left
/// out (a 36° / ~105px opening), so you drop in through the roof and stand INSIDE the bowl — the
/// cockpit. PEN material at PEN's own density -- a real body (plans/body-unify.md step 6,
/// "unpark the ship"), not the old density-0 "not a body" sentinel -- with `gravity_scale`
/// zeroed so it never falls; thrust (`ship::booster_blast`'s reaction impulse) is its only
/// force. `spin_scale` is zeroed too: an off-center strike / billiard graze never turns the
/// hull, so the hatch stays up and the station anchors stay put (2026-07-04 playtest: "why is
/// ship rotating its inks"). `owner: -1` (never expires) and `pos: SHIP_HOME` (baked in place,
/// not built via `finalize_path`, so the reference point stays exactly the geometric center
/// every station anchor / test fixture keys off, rather than the open polyline's slightly-off
/// centroid) are unchanged from the parked hull.
///
/// The hull is a CONTAINER (2026-07-04 playtest: crew "randomly clip out of blue things on the
/// weird sloping parts"): `solid = true`, `gate_side = Off`, so every rim segment blocks BOTH
/// ways -- no held-down drop through the bowl bottom, no hitstun-slide egress through the
/// shoulder. The step-5 `PassBackward` experiment (exit-only rim) is retired: the HATCH is the
/// one door, in and out. Lateral tunneling protection for the sloped Floor-class rim segments
/// comes from `body::sweep_gated_floor_lateral`, which sweeps Solid ink-owned floors as well as
/// OneWay ones (same dome-shoulder bug, now blocked from inside too). The hull is NOT zone
/// material: a same-day experiment made it carry the blast zone in flight, and the playtest
/// verdict was "wonky" -- instead the blast frame is a set of hard walls the flying hull
/// BOUNCES off (`integrate_ink`'s baked-body blast-wall reflect), so the ship and its crew
/// simply never leave the arena.
// parity(v1-ship-hull-geometry): bake_ship walks the literal rim around the omitted hatch vertex and preserves SHIP_HOME as its root
pub fn bake_ship(nodes: &mut [InkNode], free: &mut FreeSpans) -> InkPath {
    let mut p = InkPath::EMPTY;
    p.props = StrokeProps::PEN;
    p.props.gravity_scale = 0.0; // the row: thrust-only flight, never a fall (director's call, zero-g)
    p.props.spin_scale = 0.0; // the row: strikes/grazes never rotate the hull
    // 2026-07-07 playtest ("blue treated as soft platform, i choose to drop in"): the hull's Floors
    // are SOFT, like any drop-through platform -- land on top from above, hold down to fall through
    // (the hatch is no longer the only door; the dome and bowl both drop). The Wall segments (the
    // rim around the equator) are unchanged: `solid = false` only flips the Floor gate from Solid
    // to Soft in `material_of`, and `wall_gate` was already `Solid` only under the now-retired
    // `owner < 0 && solid` special case (gone in the same playtest). The rim still blocks both ways
    // via `sweep_walls` (every Wall-kind row does, regardless of `solid`). Container containment
    // across the bowl floor is forfeit by design here -- the spec is "blue = pass through".
    p.props.solid = false;
    // 2026-07-07 playtest ("top still repulses me into air fall"): the dome shoulder is sloped
    // 27°-63° (Floor-classified all the way to the equator), and the grounded-walk downslope
    // slide (`za_warudo.rs`'s `SLOPE_SLIDE_ACCEL` arm) fired on every segment past PEN's 31°
    // `floor_tol`, kicking a freshly-landed fighter down toward the equator until they ran off
    // the segment endpoint and fell -- "repelled into air fall". Raising the threshold to wall_tol
    // (~69°) means NO Floor-classified segment of the hull (all are < 69° by `classify`) ever
    // slides: you land, you stay, you can walk the dome. The flat bowl floor (~9°) was already
    // under the old threshold, which is why "the bottom mostly works" -- this extends that to
    // the top. Sliding on ordinary player strokes is unchanged (they keep PEN's 31°).
    p.props.floor_tol = StrokeProps::PEN.wall_tol;
    // open polyline: vertex 16 (right of the top gap) around the bottom to vertex 14
    // (left of the gap). Vertex 15 — the exact top — is the hole.
    let mut local = [Vector2::ZERO; MAX_PATH_PTS];
    let mut count = 0;
    let mut k = 16;
    loop {
        local[count] = SHIP_RIM[k] * SHIP_R;
        count += 1;
        if k == 14 {
            break;
        }
        k = (k + 1) % SHIP_SEGS;
    }
    bake_span(&mut p, &local[..count], SHIP_HOME, nodes, free);
    // mass = Σ|seg| · density, the same formula `finalize_path` uses -- computed by hand here
    // because `finalize_path` also RECENTERS `pos` to the node centroid, which this hull must not
    // take (see the doc comment above).
    p.mass = span_length(&p, nodes) * p.props.density;
    p
}

/// Write a baked fixture's `local` offsets into a freshly-allocated pool span, set its `{start,len}`
/// handle + `pos`, and cache its classes. Baked strokes never grow, so the span is exactly `len`
/// (no `MAX_PATH_PTS` reservation) -- `span_len` == `len` for them, same as a finalized stroke.
pub(crate) fn bake_span(
    p: &mut InkPath,
    local: &[Vector2],
    pos: Vector2,
    nodes: &mut [InkNode],
    free: &mut FreeSpans,
) {
    let start = free
        .alloc(local.len() as u16)
        .expect("baked fixture span fits the fresh pool");
    p.start = start;
    p.len = local.len() as u8;
    for (offset, &pt) in local.iter().enumerate() {
        nodes[start as usize + offset] = InkNode {
            pt,
            born_off: 0,
            class: SegClass::None,
        };
    }
    p.pos = pos;
    classify(p, nodes);
}

/// Σ|seg| over a path's span (the stroke length the mass formula scales by density).
fn span_length(p: &InkPath, nodes: &[InkNode]) -> f32 {
    let start = p.start as usize;
    let n = p.len as usize;
    (0..n.saturating_sub(1))
        .map(|s| (nodes[start + s + 1].pt - nodes[start + s].pt).length())
        .sum()
}

// ── Drawn ink paths (Kirby-Canvas-Curse-style) ────────────────────────────────────────────────────
// A drawn path AND a stage are the same primitive: a polyline (curves flattened to segments before
// the sim ever sees them). The deterministic tick only touches segments, reusing geo.rs's segment
// math, so this stays pure with ZERO new crates. SVG authoring (usvg+lyon) would be an OFFLINE bake
// that emits these same points — never in the tick — so it can't break determinism. See the
// `ink-paths` skill for the full architecture.
pub const MAX_PATH_PTS: usize = 24; // points per path → up to MAX_PATH_PTS-1 segments
pub const MAX_DRAWN: usize = 128; // simultaneous live paths (drawn ink + loaded stage strokes)
// (2026-07-04 "WAY more of everything": 12 -> 48. A 12-slot board
// starved the guns and ate landed tetris pieces; 48 first blew the
// lobby harness's stack because the net enums carried SimState BY
// VALUE -- boxed now (net/src/lobby.rs), so capacity is back.)
// (2026-07-07 48 -> 128: waste-free after the ink-storage-arena
// migration made paths 120 B handles into the shared node pool, so
// more slots no longer reserve geometry. 128 is the HARD CEILING:
// `Fighter.ground_ink: i8` (fighter.rs:30) indexes `SimState.paths`,
// so 127 is the largest valid drawn-path index and slots 0..=127
// fill the i8. 129+ needs ground_ink widened to i16 -- do not exceed.)

/// What one segment collides as. Computed ONCE at finalize by `classify` and cached on the path, so
/// the per-tick collision read is O(segments) with no trig. Grabbability lives here: a `Ledge` is a
/// `Floor` tip whose curvature (Δangle to the neighbor segment) clears `StrokeProps.ledge_curve`.
#[derive(Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Default, Debug)]
pub enum SegClass {
    #[default]
    None, // too short / a too-steep ramp: pass-through (no surface)
    Floor, // shallow enough to walk + land on (top face)
    Wall,  // near-vertical: blocks / reflects (regardless of `solid` — walls always block)
    Ledge, // a Floor tip with a sharp corner: grabbable lip
}

/// Which drawing tool an ink item is. Stored as plain data; behavior is static-dispatched through the
/// `DrawTool` trait so `SimState` never holds a trait object (stays `Copy` + checksummable).
#[derive(Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum ToolKind {
    #[default]
    TrailPen, // lays nodes along the drawer's own movement (hold attack while you run/jump)
    CursorBrush, // stick steers a cursor offset from the body; attack plants at the cursor
    StrokeRuler, // one straight stroke per press, aimed by the stick, length = remaining budget
}

/// A drawing tool's node-placement behavior. Implemented on a zero-sized marker per kind; the sim
/// calls through the `tool_sample` shim (which `match`es on `ToolKind`) so dispatch is static and
/// state stays plain data. Material is NOT a tool concern anymore — it comes from the `StrokeRegistry`
/// keyed by the item's `StrokeId`. Add a tool = add a variant + a marker impl + a `tool_sample` arm.
pub trait DrawTool {
    /// Where (if anywhere) this tool plants a new node THIS frame, given the drawer, their input, and
    /// the path so far. `None` = lay nothing this frame. The caller enforces the length budget.
    fn sample(
        f: &Fighter,
        i: &InputFrame,
        path: &InkPath,
        nodes: &[InkNode],
        t: &Tune,
    ) -> Option<Vector2>;
}

pub struct TrailPen;
pub struct CursorBrush;
pub struct StrokeRuler;

impl DrawTool for TrailPen {
    fn sample(
        f: &Fighter,
        _i: &InputFrame,
        path: &InkPath,
        nodes: &[InkNode],
        _t: &Tune,
    ) -> Option<Vector2> {
        // lay a node at the feet once we've moved at least one segment-length from the last node.
        let here = f.pos;
        match path.last(nodes) {
            Some(prev) if (here - prev).length() < path.props.min_seg => None,
            _ => Some(here),
        }
    }
}

impl DrawTool for CursorBrush {
    fn sample(
        f: &Fighter,
        i: &InputFrame,
        path: &InkPath,
        nodes: &[InkNode],
        t: &Tune,
    ) -> Option<Vector2> {
        // a cursor floats off the body in the stick direction; plant where it points.
        let aim = Vector2::new(i.dir, i.aim_y);
        let cursor = f.pos + aim * t.ink_cursor_reach;
        match path.last(nodes) {
            Some(prev) if (cursor - prev).length() < path.props.min_seg => None,
            _ => Some(cursor),
        }
    }
}

impl DrawTool for StrokeRuler {
    fn sample(
        f: &Fighter,
        i: &InputFrame,
        path: &InkPath,
        nodes: &[InkNode],
        _t: &Tune,
    ) -> Option<Vector2> {
        // straight stroke: from the body, step one min_seg in the aimed direction each frame until
        // budget runs out (the caller stops us). First node anchors at the body.
        let aim = Vector2::new(i.dir, i.aim_y);
        let dir = if aim.length() > 0.3 {
            aim / aim.length()
        } else {
            Vector2::new(f.facing, 0.0)
        };
        match path.last(nodes) {
            None => Some(f.pos),
            Some(prev) => Some(prev + dir * path.props.min_seg),
        }
    }
}

/// Static-dispatch shim: where (if anywhere) a tool plants a node this frame.
// parity(v1-ink-tool-sampling): trail pen, cursor brush, and stroke ruler each author executable point geometry through one tool dispatch
pub fn tool_sample(
    k: ToolKind,
    f: &Fighter,
    i: &InputFrame,
    p: &InkPath,
    nodes: &[InkNode],
    t: &Tune,
) -> Option<Vector2> {
    match k {
        ToolKind::TrailPen => TrailPen::sample(f, i, p, nodes, t),
        ToolKind::CursorBrush => CursorBrush::sample(f, i, p, nodes, t),
        ToolKind::StrokeRuler => StrokeRuler::sample(f, i, p, nodes, t),
    }
}

/// One drawn polyline, now a HANDLE into the shared `SimState.nodes` arena (plans/ink-storage-arena.md
/// slice 4): the geometry (`pt`/`born_off`/`class` per node) lives in `[InkNode; NODE_POOL]`, and this
/// head owns the span `[start, start + len)`. `Copy` + fixed-size so it rides inside `SimState` and
/// rolls back / checksums like everything else, but adding a slot now costs one head (~120 B), not a
/// `MAX_PATH_PTS` reservation. A DRAWING stroke holds a full `MAX_PATH_PTS` span (grows into it);
/// `finalize_path` shrinks the span to `len`, returning the tail to `SimState.free`.
///
/// Geometry is LOCAL: `nodes[start + i].pt` is an offset from `pos`; `world_pt(i, nodes)` is the only
/// world read. While drawing, `pos` stays ZERO (local == world); `finalize_path` rebases to the
/// centroid. Translation-only unless in flight (`rot`; see plans/ink-billiards.md).
// parity(v1-ink-path-arena): path heads own fixed rollback-state node spans and preserve local geometry, birth order, allocation, trimming, and release
#[derive(Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct InkPath {
    pub start: u16, // index of node 0 in `SimState.nodes`; the span is `[start, start + span_len)`
    pub len: u8,    // live node count (per-stroke cap MAX_PATH_PTS unchanged)
    pub kind: ToolKind,
    pub props: StrokeProps,
    pub owner: i8,     // who drew it (-1 = baked stage stroke, never expires/draws)
    pub drawing: bool, // true while the owner is still laying it (span == MAX_PATH_PTS)
    pub budget: f32,   // remaining length budget (px); ≤0 = finalize
    // ── rigid body (translation only) ──
    pub pos: Vector2, // reference point (centroid at finalize), world space
    pub vel: Vector2, // px/frame (ink-native); ZERO = still, nonzero = traveling
    pub percent: f32, // damage %, scales knockback taken (fighters' formula) -- an amplifier, never a gate
    pub mass: f32,    // Σ|seg|·density at finalize; 0.0 = not a body (baked/drawing)
    pub shake: i64,   // hitstun frames left from a too-weak strike (cosmetic jitter)
    pub rot: f32,     // orientation (rad) — nonzero ONLY in flight; settle bakes it
    pub omega: f32,   // angular vel (rad/frame): spin from an off-center hit / the lob
    // ── anchored drawing (the ink gun) ──
    pub anchor: i8, // fighter idx the mid-draw path rides (-1 = free; pens draw free at pos ZERO)
    pub anchor_dir: Vector2, // unit dir from the drawer to the anchor point, fixed at A-press;
    // release fires the finished shape along it (TODO: slingshot-by-swipe)
    // ── birth tick base (per-node offsets now live in `InkNode.born_off`) ──
    pub stroke_born: u64, // this stroke's base tick; NOT rebased when trim_front drops node 0 (the
    // remaining offsets stay correct against the unchanged base — see `node_born`)
    pub cell: Option<crate::v1::terrain_cells::TerrainCell>,
}

impl InkPath {
    pub const EMPTY: Self = Self {
        cell: None,
        start: 0,
        len: 0,
        kind: ToolKind::TrailPen,
        props: StrokeProps::PEN,
        owner: -1,
        drawing: false,
        budget: 0.0,
        pos: Vector2::ZERO,
        vel: Vector2::ZERO,
        percent: 0.0,
        mass: 0.0,
        shake: 0,
        rot: 0.0,
        omega: 0.0,
        anchor: -1,
        anchor_dir: Vector2::ZERO,
        stroke_born: 0,
    };

    pub fn active(&self) -> bool {
        self.len > 0
    }

    /// This path's node span in the pool: `MAX_PATH_PTS` while drawing (grows into it), else `len`
    /// (finalized/baked spans are shrunk to fit). The value `free` must reclaim on death.
    #[inline]
    fn span_len(&self) -> usize {
        if self.drawing {
            MAX_PATH_PTS
        } else {
            self.len as usize
        }
    }

    /// Local offset of node `i` (pool read; the `world_pt` primitive).
    #[inline]
    fn local(&self, i: usize, nodes: &[InkNode]) -> Vector2 {
        nodes[self.start as usize + i].pt
    }

    /// Class of the segment STARTING at node `i` (cached by `classify`).
    #[inline]
    pub fn seg_class(&self, i: usize, nodes: &[InkNode]) -> SegClass {
        nodes[self.start as usize + i].class
    }

    /// Absolute tick node `i` was laid: the stroke's base tick plus its stored offset.
    /// The only way to read a node's birth tick — `born_off` alone is relative and meaningless.
    #[inline]
    pub fn node_born(&self, i: usize, nodes: &[InkNode]) -> u64 {
        self.stroke_born + nodes[self.start as usize + i].born_off as u64
    }

    /// World-space node `i`. The ONLY way to read a node as world coords. `rot` spins the local
    /// offsets about `pos`; it is 0 for everything except a body in flight, so the settled/drawn
    /// hot path stays the plain add.
    #[inline]
    pub fn world_pt(&self, i: usize, nodes: &[InkNode]) -> Vector2 {
        let p = self.local(i, nodes);
        if self.rot == 0.0 {
            return p + self.pos;
        }
        let (s, c) = self.rot.sin_cos();
        Vector2::new(p.x * c - p.y * s, p.x * s + p.y * c) + self.pos
    }

    /// World-space segment starting at node `i`. The collision / hurtbox primitive.
    #[inline]
    pub fn world_seg(&self, i: usize, nodes: &[InkNode]) -> (Vector2, Vector2) {
        (self.world_pt(i, nodes), self.world_pt(i + 1, nodes))
    }

    /// Bounding circle for coarse (billiard) passes: `pos` + the farthest local node.
    /// No thickness pad yet — the capsule radius arrives with the hurtbox work.
    pub fn bound_circle(&self, nodes: &[InkNode]) -> (Vector2, f32) {
        let start = self.start as usize;
        let r = nodes[start..start + self.len as usize]
            .iter()
            .map(|node| node.pt.length())
            .fold(0.0, f32::max);
        (self.pos, r)
    }

    /// Moment of inertia about the centroid, derived (never stored, no schema bump):
    /// point-mass approximation with the mass shared evenly across the nodes at their
    /// local offsets. Rotation-invariant, so flight `rot` needs no correction.
    pub fn inertia(&self, nodes: &[InkNode]) -> f32 {
        let n = self.len as usize;
        if n == 0 || self.mass <= 0.0 {
            return 1.0;
        }
        let start = self.start as usize;
        let sum: f32 = nodes[start..start + n]
            .iter()
            .map(|node| node.pt.length_squared())
            .sum();
        (self.mass * sum / n as f32).max(1.0)
    }

    /// Projection into the impulse solver (plans/body-bus.md). Baked stage (mass 0)
    /// projects immovable. Units are px/frame — ink's native velocity.
    pub(crate) fn body_bits(&self, nodes: &[InkNode]) -> BodyBits {
        let movable = self.mass > 0.0;
        BodyBits {
            pos: self.pos,
            vel: self.vel,
            omega: self.omega,
            inv_mass: if movable { 1.0 / self.mass } else { 0.0 },
            inv_inertia: if movable {
                1.0 / self.inertia(nodes)
            } else {
                0.0
            },
        }
    }

    /// Airborne body? Baked stage (`mass == 0`) never travels.
    #[inline]
    pub fn traveling(&self) -> bool {
        self.mass > 0.0 && self.vel != Vector2::ZERO
    }

    /// The most recently laid node (world space), if any.
    pub fn last(&self, nodes: &[InkNode]) -> Option<Vector2> {
        (self.len > 0).then(|| self.world_pt(self.len as usize - 1, nodes))
    }

    /// Append a node (world coords in; stored as a `pos`-relative offset) into the pre-allocated
    /// span. Drops the oldest if full. Records its birth tick for per-node expiry. The span
    /// (`[start, start + MAX_PATH_PTS)`) must already be allocated by the stroke's creator.
    pub(crate) fn push(&mut self, p: Vector2, tick: u64, nodes: &mut [InkNode]) {
        if self.len as usize == MAX_PATH_PTS {
            self.trim_front(1, nodes);
        }
        let n = self.len as usize;
        let slot = self.start as usize + n;
        nodes[slot].pt = p - self.pos;
        if n == 0 {
            // first live node: it defines this stroke's base tick, offset zero by definition.
            self.stroke_born = tick;
            nodes[slot].born_off = 0;
        } else {
            // safe: strokes are bounded by stroke_life (u16 frames ~= 18 min @60fps); baked
            // stage strokes never reach this per-node lay path (drawn once, then owner < 0).
            nodes[slot].born_off = (tick - self.stroke_born) as u16;
        }
        self.len += 1;
    }

    /// Drop the `k` oldest nodes (front), shifting the rest down WITHIN the fixed span (`start`
    /// stays put, so the drawing reservation never overruns its allocation). Keeps indexing trivial.
    /// `stroke_born` is deliberately NOT rebased even though node 0 (its offset-zero anchor)
    /// may be among the dropped: the remaining offsets are still correct against the old base,
    /// and rebasing would mean rewriting every remaining offset for no benefit.
    pub(crate) fn trim_front(&mut self, k: usize, nodes: &mut [InkNode]) {
        let k = k.min(self.len as usize);
        if k == 0 {
            return;
        }
        let start = self.start as usize;
        let live = self.len as usize - k;
        for i in 0..live {
            nodes[start + i] = nodes[start + i + k];
        }
        self.len = live as u8;
    }

    /// Return this path's node span to the pool and blank the head -- the ONE death primitive
    /// (expiry / prune / eviction / blast). A DRAWING stroke holds `MAX_PATH_PTS`, a settled one its
    /// `len`; either way `free.free` is a pure fn of state, so two peers reclaim identically.
    pub(crate) fn release(&mut self, free: &mut FreeSpans) {
        let span = self.span_len();
        if span > 0 {
            free.free(self.start, span as u16);
        }
        *self = InkPath::EMPTY;
    }
}

/// Classify every segment of a path ONCE (at finalize, or after a node expires). This is the cached
/// grabbability the per-tick collision read consumes: each `class[i]` is the surface of the segment
/// starting at node `i`. Floor/Wall by slope tolerance; a Floor tip becomes a grabbable `Ledge` where
/// the turn to the neighbor segment is sharp enough (curvature ≥ `ledge_curve`). Pure, no per-frame
/// trig once cached.
// parity(v1-ink-surface-classification): finalized segments become floor, wall, ledge, or none from material tolerances, length, winding, and neighboring curvature
pub fn classify(p: &InkPath, nodes: &mut [InkNode]) {
    let n = p.len as usize;
    let start = p.start as usize;
    // Only the live span is cleared: a shrunk finalized span shares the pool with other allocations,
    // so writing past `len` would corrupt a neighbor. `class[i]` is meaningful for `i < len - 1`.
    for offset in 0..n {
        nodes[start + offset].class = SegClass::None;
    }
    if n < 2 {
        return;
    }
    // base pass: each segment is Floor / Wall / None by its own slope. A `force_wall` stroke (the wall
    // pen) skips the slope test entirely — every real segment is Wall, so there are no hollow bits and
    // no grabbable lips (the ledge pass below is a no-op since nothing classifies as Floor). Reads are
    // pulled into locals before the write so the pool's shared borrow is released first.
    for s in 0..n - 1 {
        let d = nodes[start + s + 1].pt - nodes[start + s].pt;
        let slope = geo::slope_about(d, geo::DOWN);
        nodes[start + s].class = if d.length() < p.props.min_seg {
            SegClass::None
        } else if p.props.force_wall {
            SegClass::Wall
        } else if slope >= p.props.wall_tol {
            SegClass::Wall
        } else {
            // Flat OR mid-slope: a Floor either way. Flat you stand on; a ramp between floor_tol and
            // wall_tol is a sloped Floor you stand/slide on — NOT a hole. `floor_tol` now only splits
            // "flat" from "sloped" for the slide accel (see the grounded-ink branch), not floor vs void.
            SegClass::Floor
        };
    }
    // ledge pass: a Floor segment whose join to the NEXT segment turns sharply (a corner, not a smooth
    // continuation) is grabbable. The two end Floor segments are also candidate lips (open ends) —
    // unless the path is a CLOSED loop (last node back on the first, e.g. a tetromino outline),
    // where "first" and "last" are an arbitrary seam, not real tips.
    let closed = n >= 3 && (nodes[start].pt - nodes[start + n - 1].pt).length() < 0.01;
    for s in 0..n - 1 {
        if nodes[start + s].class != SegClass::Floor {
            continue;
        }
        let seg0 = nodes[start + s].pt;
        let seg1 = nodes[start + s + 1].pt;
        let a = (seg1 - seg0).y.atan2((seg1 - seg0).x);
        let open_end = !closed && (s == 0 || s == n - 2);
        let corner = if s + 2 < n {
            let seg2 = nodes[start + s + 2].pt;
            let b = (seg2 - seg1).y.atan2((seg2 - seg1).x);
            ang_diff(a, b) >= p.props.ledge_curve
        } else {
            false
        };
        if open_end || corner {
            nodes[start + s].class = SegClass::Ledge;
        }
    }
}

/// y of the highest walkable (`Floor`/`Ledge`) segment of `p` directly under world-x `x`, or `None`
/// when no walkable segment spans `x`. Reads the cached `class[]` only (no trig); linear-interpolates
/// y across the spanning segment. A path still being drawn isn't yet collidable. This is the per-tick
/// collision read both the landing scan and the grounded pin consume.
pub(crate) fn ink_floor_y_at(p: &InkPath, x: f32, nodes: &[InkNode]) -> Option<f32> {
    if !p.active() || p.drawing {
        return None;
    }
    let n = p.len as usize;
    let mut best: Option<f32> = None;
    for s in 0..n.saturating_sub(1) {
        if !matches!(p.seg_class(s, nodes), SegClass::Floor | SegClass::Ledge) {
            continue;
        }
        let (a, b) = p.world_seg(s, nodes);
        let (lo, hi) = if a.x <= b.x { (a, b) } else { (b, a) };
        if x < lo.x || x > hi.x {
            continue;
        }
        let span = hi.x - lo.x;
        let y = if span < 1e-3 {
            lo.y.min(hi.y)
        } else {
            lo.y + (hi.y - lo.y) * (x - lo.x) / span
        };
        best = Some(best.map_or(y, |by| by.min(y))); // smaller y = higher surface
    }
    best
}

/// y of the walkable (`Floor`/`Ledge`) segment of `p` spanning world-x `x` whose y sits CLOSEST to
/// `ref_y`, or `None` when no walkable segment spans `x`. Same scan as `ink_floor_y_at`, but picks
/// the candidate nearest a known reference height instead of the topmost one. The grounded-ink pin
/// needs "the floor I'm already standing on", not "the highest floor anywhere above my feet" — on a
/// closed hull stroke (the ship) the dome overhead and the bowl floor underneath can both span the
/// same x, and `ink_floor_y_at` always hands back the dome.
pub(crate) fn ink_floor_y_near(p: &InkPath, x: f32, ref_y: f32, nodes: &[InkNode]) -> Option<f32> {
    if !p.active() || p.drawing {
        return None;
    }
    let n = p.len as usize;
    let mut best: Option<f32> = None;
    for s in 0..n.saturating_sub(1) {
        if !matches!(p.seg_class(s, nodes), SegClass::Floor | SegClass::Ledge) {
            continue;
        }
        let (a, b) = p.world_seg(s, nodes);
        let (lo, hi) = if a.x <= b.x { (a, b) } else { (b, a) };
        if x < lo.x || x > hi.x {
            continue;
        }
        let span = hi.x - lo.x;
        let y = if span < 1e-3 {
            lo.y.min(hi.y)
        } else {
            lo.y + (hi.y - lo.y) * (x - lo.x) / span
        };
        best = Some(match best {
            None => y,
            Some(by) if (y - ref_y).abs() < (by - ref_y).abs() => y,
            Some(by) => by,
        });
    }
    best
}

/// Smallest absolute angle between two headings (radians), in 0..π.
fn ang_diff(a: f32, b: f32) -> f32 {
    let mut d = (a - b).abs() % (std::f32::consts::TAU);
    if d > std::f32::consts::PI {
        d = std::f32::consts::TAU - d;
    }
    d
}

/// How far the ink gun's drawing cursor swings off its anchor point at full c-stick (px).
pub const INK_DRAW_RANGE: f32 = 120.0;
/// Drawing back within this of the start dot snaps an anchored shape into a closed loop (px).
pub const INK_CLOSE_R: f32 = 12.0;
/// Muzzle speed of a released drawn shape (px/s; converted to px/frame at fire).
pub const INK_SHOT_SPEED: f32 = 900.0;

/// Release (or budget-out) of an ANCHORED shape fires it: momentum along the locked
/// anchor direction plus the drawer's own velocity, so a moving fighter slings it.
/// Free pen strokes (`anchor < 0`) are untouched. Call after `finalize_path`.
/// TODO(slingshot): read a release-frame c-stick swipe to bend the shot off `anchor_dir`.
fn fire_drawn(p: &mut InkPath, f_vel: Vector2) {
    if p.anchor < 0 {
        return;
    }
    p.vel = (p.anchor_dir * INK_SHOT_SPEED + f_vel) * DT;
    p.anchor = -1;
}

/// Post-step ink: lay/extend each drawing fighter's path (tool-specific, budget-capped), finalize a
/// path the moment its owner stops drawing or runs out of budget (running `classify` once to cache
/// grabbability), then decay old nodes per-node and free spent slots. Baked stage strokes (owner < 0)
/// never draw or decay. Pure; the only place ink paths mutate. Called last in `step`. See the
/// `ink-paths` skill.
// parity(v1-ink-path-authoring): tool sampling, toggle start and finish, anchored loop drawing, gas budget, launch interruption, and board allocation all mutate paths here
pub(crate) fn update_paths(n: &mut SimState, inputs: &[&InputFrame], t: &Tune) {
    let tick = n.tick;
    let np = (n.active as usize).min(inputs.len());
    for idx in 0..np {
        let f = n.fighters[idx];
        let holding = f.holding;
        let logic =
            (holding >= 0).then(|| crate::v1::item::item_logic(n.items[holding as usize].kind));
        let draws = logic.is_some_and(|l| l.draws);
        // aims AND draws = the ink gun: the mid-draw shape anchors to the hand and fires on release
        let anchored = logic.is_some_and(|l| l.draws && l.aims);
        let inp = inputs[idx];
        // Paint is a TOGGLE: one attack press starts the stroke, the next ends it (and
        // fires an anchored shape). Holding the button pinned the thumb and blocked
        // jump/special — toggling frees the hand while painting. pickup_hold keeps the
        // press that claimed the tool from also being the press that starts painting.
        let toggle = draws && inp.attack && !f.pickup_hold;
        let active_slot = n
            .paths
            .iter()
            .position(|p| p.drawing && p.owner == idx as i8);
        // mid-draw hit: getting launched rips an anchored shape loose — it survives at half
        // mass and flies back at its drawer. High stakes. (TODO: flip owner to the hitter
        // once last-hitter attribution exists, so the returned shape scores for them.)
        if f.hitstun > 0 {
            if let Some(s) = active_slot {
                if n.paths[s].anchor >= 0 {
                    let back = (f.pos - n.paths[s].pos).normalize_or_zero();
                    let dir = if back == Vector2::ZERO {
                        -n.paths[s].anchor_dir
                    } else {
                        back
                    };
                    finalize_path(&mut n.paths[s], &mut n.nodes, &mut n.free);
                    n.paths[s].mass *= 0.5;
                    n.paths[s].vel = dir * INK_SHOT_SPEED * DT;
                    n.paths[s].anchor = -1;
                } else {
                    finalize_path(&mut n.paths[s], &mut n.nodes, &mut n.free); // free pen stroke just solidifies
                }
            }
            continue;
        }
        let slot = match active_slot {
            Some(s) => {
                if toggle || !draws {
                    // second press — or the tool left the hand (thrown/dropped/spent) —
                    // ends the stroke; an anchored shape fires on the way out.
                    finalize_path(&mut n.paths[s], &mut n.nodes, &mut n.free);
                    fire_drawn(&mut n.paths[s], f.vel);
                    continue;
                }
                s // still painting: keep laying, no button required
            }
            None => {
                if !toggle {
                    continue;
                }
                let tool = n.items[holding as usize].tool;
                {
                    // full board: shared eviction policy (board.rs) instead of silently
                    // dropping the stroke -- the "gun sometimes doesn't emit" bug.
                    let Some(s) = board::claim_stroke_slot(&n.paths, &n.nodes) else {
                        continue; // every slot untouchable — keep the ink, no stroke
                    };
                    n.paths[s].release(&mut n.free); // evicted: reclaim the loser's span first (no-op on a free slot)
                    // reserve a full drawing span up front (plans/ink-storage-arena.md slice 4); a
                    // full pool evicts ANOTHER stroke to make room, else the shot never happens.
                    let Some(start) = alloc_draw_span(&mut n.paths, &n.nodes, &mut n.free) else {
                        continue;
                    };
                    let mut fresh = InkPath::EMPTY;
                    fresh.start = start;
                    fresh.kind = tool;
                    fresh.props = t.strokes.get(n.items[holding as usize].stroke); // registry lookup by StrokeId
                    fresh.owner = idx as i8;
                    fresh.drawing = true;
                    // Each stroke draws from the pen's remaining gas (its total ink), not a fresh
                    // per-stroke budget, so gas depletes across strokes and the overhead bar means
                    // "ink left". The pen is spent (vanishes) when gas hits zero, like a gun.
                    fresh.budget = n.items[holding as usize].gas;
                    if anchored {
                        // the anchor direction locks at A-press: c-stick if deflected, else facing.
                        let c = Vector2::new(inp.cx, inp.cy);
                        fresh.anchor = idx as i8;
                        fresh.anchor_dir = if c.length() >= 0.4 {
                            c.normalize_or_zero()
                        } else {
                            Vector2::new(f.facing, 0.0)
                        };
                        fresh.pos = f.pos + fresh.anchor_dir * t.ink_cursor_reach;
                    }
                    n.paths[s] = fresh;
                    s
                }
            }
        };
        {
            let plant = if n.paths[slot].anchor >= 0 {
                // anchored: `pos` rides the hand each frame (pts are offsets, so the whole
                // half-drawn shape moves with the fighter); the c-stick steers the drawing
                // cursor around that anchor point.
                let ap = f.pos + n.paths[slot].anchor_dir * t.ink_cursor_reach;
                n.paths[slot].pos = ap;
                // cursor input: the c-stick when deflected, else the MAIN stick — holding A
                // occupies the right thumb on a pad (and a keyboard may have no c-stick), so
                // the main stick must be able to steer the drawing. Moving the fighter only
                // translates the anchored shape; the stick offset is what lays geometry.
                let cs = Vector2::new(inp.cx, inp.cy);
                let stick = if cs.length() >= 0.2 {
                    cs
                } else {
                    Vector2::new(inp.dir, inp.aim_y)
                };
                let cursor = ap + stick * INK_DRAW_RANGE;
                let path = &n.paths[slot];
                if path.len >= 3 && (cursor - path.world_pt(0, &n.nodes)).length() <= INK_CLOSE_R {
                    // back at the start dot: snap the loop shut
                    let start = path.world_pt(0, &n.nodes);
                    path.last(&n.nodes).and_then(|prev| {
                        ((start - prev).length() >= path.props.min_seg).then_some(start)
                    })
                } else {
                    match path.last(&n.nodes) {
                        Some(prev) if (cursor - prev).length() < path.props.min_seg => None,
                        _ => Some(cursor),
                    }
                }
            } else {
                tool_sample(n.paths[slot].kind, &f, inp, &n.paths[slot], &n.nodes, t)
            };
            if let Some(p) = plant {
                let path = n.paths[slot];
                let add = path.last(&n.nodes).map_or(0.0, |prev| (p - prev).length());
                if path.len == 0 || (add > 0.0 && path.budget - add >= 0.0) {
                    n.paths[slot].push(p, tick, &mut n.nodes);
                    n.paths[slot].budget -= add;
                    n.items[holding as usize].gas -= add; // deplete the pen's total gas (HUD bar)
                }
                if n.paths[slot].budget <= 0.0 {
                    finalize_path(&mut n.paths[slot], &mut n.nodes, &mut n.free); // stroke's gas spent: solidify
                    fire_drawn(&mut n.paths[slot], f.vel); // an anchored shape fires even on empty
                }
                if n.items[holding as usize].gas <= 0.0 {
                    // pen out of ink: don't pop instantly like a spent gun — detach it to the ground
                    // (still briefly pickup-able). `update_items` despawns it once it settles idle on
                    // the floor with no ink left (its "unload").
                    n.items[holding as usize].owner = -1;
                    n.fighters[idx].holding = -1;
                }
            }
        }
    }

    // parity(v1-ink-decay-release): finished authored strokes expire as a whole from newest-node age while baked, drawing, and permanent rows remain exempt
    // whole-stroke decay: a FINISHED stroke lives `stroke_life` frames past its last-laid node, then
    // exits all at once (the shell shows the countdown above it). Still-drawing strokes and baked
    // stage strokes (owner < 0) never expire. The timer is anchored on the newest node's birth, so it
    // starts counting from the moment the owner stops extending the stroke.
    for slot in 0..MAX_DRAWN {
        if n.paths[slot].shake > 0 {
            n.paths[slot].shake -= 1; // struck-ink hitstun countdown (shell jitters while > 0)
        }
        let p = &n.paths[slot];
        if !p.active() || p.owner < 0 || p.drawing || p.props.stroke_life < 0 {
            continue; // stroke_life < 0 = the never-expires sentinel (tetris pen, later: baked stage)
        }
        let newest = p.node_born(p.len as usize - 1, &n.nodes);
        if tick.saturating_sub(newest) > p.props.stroke_life as u64 {
            n.paths[slot].release(&mut n.free);
        }
    }
}

/// Reserve a full `MAX_PATH_PTS` drawing span for a new stroke. Pool-full evicts the oldest
/// EVICTABLE player stroke (same age order as `claim_stroke_slot`), reclaims its span, and retries;
/// `None` only when nothing is evictable (baked/drawing/traveling everywhere) -- caller keeps its ink.
/// Deterministic: `free.alloc` and `board::oldest_evictable` are both pure fns of state.
fn alloc_draw_span(
    paths: &mut [InkPath; MAX_DRAWN],
    nodes: &[InkNode],
    free: &mut FreeSpans,
) -> Option<u16> {
    loop {
        if let Some(start) = free.alloc(MAX_PATH_PTS as u16) {
            return Some(start);
        }
        let victim = board::oldest_evictable(paths, nodes)?;
        paths[victim].release(free);
    }
}

/// Ink gravity/terminal scale vs fighters: pieces hang in the air (the floaty stage hazard).
pub const INK_FLOAT: f32 = 0.35;
/// A traveling body at or above this speed (px/frame) is a live hazard: it slams any fighter it
/// touches — INCLUDING its maker. Below it, it drifts harmlessly.
pub const INK_TRUCK_SPEED: f32 = 5.0;
/// Faster than this into a floor (px/frame) the body was, historically, a lively impact; at or
/// below it the impact is "slow" -- the first half of the settle/lock override predicate. Kept as
/// the approach-speed gate so a restitution-0 material locks on exactly the frames it used to.
pub const INK_SETTLE_SPEED: f32 = 3.5;
/// Below this rebound speed (px/frame) the generic solve's bounce is imperceptible, so the settle
/// override treats the body as done rebounding and bakes it. A restitution-0 material rebounds at
/// exactly 0 -> always at-or-below -> it locks precisely as the old hardcoded slow-branch did
/// (byte-identical). A restitution>0 material keeps hopping off the stage until its rebound decays
/// under this floor, which is the one intended behavior change (plans/body-unify.md step 2).
pub const INK_LOCK_REBOUND: f32 = 0.5;
/// Sideways speed kept through a bounce (ground friction of the hop).
pub const INK_BOUNCE_FRICTION: f32 = 0.8;
/// Spin picked up rolling out of a bounce: omega per px/frame of ground speed.
pub const INK_ROLL_SPIN: f32 = 0.02;
/// Air drag on spin per frame (pure feel: the ball slows its tumble).
pub const INK_SPIN_DAMP: f32 = 0.995;
/// Hardest allowed tumble (rad/frame) off a strike, either direction.
pub const INK_MAX_SPIN: f32 = 0.35;

/// Traveling-ink integrator: gravity arc + tumble, then either BOUNCE off a surface top it crosses
/// — a platform, OR another still stroke's walkable face (`others[me]` is skipped), so lobbed
/// tetris pieces STACK — or settle (the lock) once the impact is slow. `rot` spins the world reads
/// in flight; the lock bakes it into `pts` and re-classifies, so settled ink is axis-fixed again
/// and everything downstream (standing, persist, strike) sees plain geometry. Reuses the fighters'
/// gravity/terminal so ink and bodies share one feel.
// parity(v1-ink-flight-bounce-settle): traveling strokes arc, spin, collide with platform and ink tops, bounce by material, then bake and reclassify only after the rebound settles
pub(crate) fn integrate_ink(
    p: &mut InkPath,
    others: &[InkPath; MAX_DRAWN],
    me: usize,
    nodes: &mut [InkNode],
    free: &mut FreeSpans,
    t: &Tune,
) {
    if !p.traveling() {
        return;
    }
    // ink vel is px/FRAME: convert the fighters' px/s² gravity and px/s terminal. (Un-converted,
    // gravity slammed vel to terminal in ONE frame — the "lob lands flat instantly" bug.)
    // INK_FLOAT then softens both: pieces hang and drift, fighters keep their weight.
    // `gravity_scale` (plans/body-unify.md step 6) is the one row every stroke's fall passes
    // through: 1.0 (every preset but the ship hull) reproduces this exactly; the hull's row is
    // 0.0, so it never falls -- thrust is its only force. No owner/slot branch here, ever.
    p.vel.y = (p.vel.y + t.gravity * DT * DT * INK_FLOAT * p.props.gravity_scale)
        .min(t.max_fall * DT * INK_FLOAT);
    p.pos += p.vel;
    // `spin_scale` is the tumble twin of `gravity_scale` (same row-not-special-case rule):
    // applied BEFORE the accrual so a 0.0 row (the hull) never turns, whoever wrote `omega`
    // this frame (strike absorb, billiard writeback, the roll below). 1.0 is byte-identical.
    p.omega *= p.props.spin_scale;
    p.rot += p.omega;
    p.omega *= INK_SPIN_DAMP;
    // `owner < 0` never expires (the hull's own doc comment) -- true already for `prune_outside`,
    // now also here: giving the hull mass (step 6) makes it strikeable (`PunchableFace::guard`
    // reads `mass <= 0`), so a hard enough hit could otherwise launch it past this line and wipe
    // it permanently, with no respawn unlike a fighter. A home-return/tether is still an open
    // question (plans/lovers-ship.md "Unparking" item 4); this only stops the outright deletion.
    if p.pos.y > BLAST_Y && p.owner >= 0 {
        p.release(free); // fell off the world mid-flight: dies like a launched fighter
        return;
    }
    // BAKED bodies (owner < 0: the hull) treat the blast frame as HARD WALLS and bounce off
    // them (2026-07-04 director's call: the ship never leaves the arena; the shell paints the
    // same frame red so the boundary is visible). Per-edge clamp + reflect with the material's
    // own restitution row -- axis-aligned walls, so the reflect is one component per edge.
    if p.owner < 0 {
        let (center, radius) = p.bound_circle(nodes);
        let bounce = p.props.bounce;
        if center.x - radius < BLAST_LEFT && p.vel.x < 0.0 {
            p.pos.x += BLAST_LEFT - (center.x - radius);
            p.vel.x = -p.vel.x * bounce;
        }
        if center.x + radius > BLAST_RIGHT && p.vel.x > 0.0 {
            p.pos.x -= (center.x + radius) - BLAST_RIGHT;
            p.vel.x = -p.vel.x * bounce;
        }
        if center.y - radius < BLAST_TOP && p.vel.y < 0.0 {
            p.pos.y += BLAST_TOP - (center.y - radius);
            p.vel.y = -p.vel.y * bounce;
        }
        if center.y + radius > BLAST_Y && p.vel.y > 0.0 {
            p.pos.y -= (center.y + radius) - BLAST_Y;
            p.vel.y = -p.vel.y * bounce;
        }
    }
    if p.vel.y <= 0.0 {
        return; // rising ink can't land
    }
    // settle test: a node crossed a surface TOP this frame (prev above-or-on, now on-or-below).
    // The main floor is PLATFORMS[0], so off-stage ink keeps falling — no infinite ground plane.
    // Track the DEEPEST penetration so the snap leaves the lowest crossing node on its surface.
    let mut snap: f32 = -1.0;
    for i in 0..p.len as usize {
        let w = p.world_pt(i, nodes);
        let prev_y = w.y - p.vel.y;
        for pl in &PLATFORMS {
            if w.x >= pl.left && w.x <= pl.right && prev_y <= pl.y && w.y >= pl.y {
                snap = snap.max(w.y - pl.y);
            }
        }
        // still ink is landable terrain too (the stack): crossing another stroke's highest
        // walkable face under this node locks exactly like a platform top.
        for (j, q) in others.iter().enumerate() {
            if j == me || !q.active() || q.drawing || q.traveling() {
                continue;
            }
            if let Some(fy) = ink_floor_y_at(q, w.x, nodes) {
                if prev_y <= fy && w.y >= fy {
                    snap = snap.max(w.y - fy);
                }
            }
        }
    }
    if snap >= 0.0 {
        p.pos.y -= snap;
        // The floor top the node crossed: its outward normal is up (opposite gravity). Every snap
        // here is a floor-top crossing, so the contact normal is `-DOWN` by construction.
        let up = -geo::DOWN;
        let along = up.perp(); // the surface tangent direction (unit)
        // Approach speed INTO the floor, captured before the solve -- the old `p.vel.y` gate,
        // written as a projection so the predicate stays dimension-disciplined.
        let approach_speed = p.vel.dot(geo::DOWN);

        // GENERIC SOLVE (always runs, never bypassed): the collide-family normal reflection with
        // the material's restitution, treating the stage/baked surface as infinite mass (a fixed
        // reflector is exactly `collide()` against inv_mass 0). Ink's own tangential feel -- ground
        // friction on the along-surface component + the roll spin it couples to -- is layered on
        // top; for restitution 0 this reproduces the old hop/roll velocity byte-for-byte.
        let restitution = contact::material_of(&p.props).restitution;
        let reflected = geo::reflect(p.vel, up, restitution);
        let along_speed = reflected.dot(along);
        let normal_vel = reflected - along * along_speed;
        p.vel = normal_vel + along * (along_speed * INK_BOUNCE_FRICTION);
        p.omega = along_speed * INK_BOUNCE_FRICTION * INK_ROLL_SPIN;

        // OVERRIDE PREDICATE (settle/lock), applied AFTER the solve: slow + still (done rebounding)
        // + floor-ish normal -> bake into terrain. `rebound_speed` is the solved away-from-floor
        // speed (`restitution * approach_speed` against the immovable surface). For restitution 0
        // rebound is 0, so this is exactly the old `approach_speed <= INK_SETTLE_SPEED` slow gate;
        // a restitution>0 body instead keeps hopping until its rebound decays under the floor.
        let rebound_speed = restitution * approach_speed;
        let slow_impact = approach_speed <= INK_SETTLE_SPEED;
        let done_rebounding = rebound_speed <= INK_LOCK_REBOUND;
        let floor_ish = up.dot(-geo::DOWN) > 0.0;
        if slow_impact && done_rebounding && floor_ish {
            billiard::bake_rotation(p, nodes);
            p.vel = Vector2::ZERO; // locked: Still IS the cluster
            p.omega = 0.0;
        }
    }
}

/// Effective half-thickness of an ink line for ink↔ink contact (segments are zero-width
/// for everything else; two lines "touch" within twice this).
// parity(v1-ink-contact-thickness): ink-to-ink narrow phase treats every segment as a capsule chain with the executable stroke radius
pub(crate) const INK_BODY_R: f32 = 6.0;
/// Ink-on-ink Coulomb friction: what sheds a glancing blow into spin transfer.
pub(crate) const INK_MU: f32 = 0.4;
// ── tetromino bodies (the lobbed "blocky boy": a closed ink shape, fired not drawn) ──────────────

/// Tetromino cell size in px. 50 makes the O piece 100×100 — character-sized.
pub const TETRIS_CELL: f32 = 50.0;
/// How many shapes `tetromino_path` knows (index with `shape % TETROMINO_SHAPES`).
pub const TETROMINO_SHAPES: u8 = 5;

/// Closed outline vertices per shape, in cell units, y-down. The path closes by re-pushing the
/// first vertex, so every side is a real collision segment (top = Floor to stand on, sides = Wall).
// parity(v1-tetromino-outline-geometry): tetromino_outline and tetromino_path define every closed piece and its finalize-time root shift
fn tetromino_outline(shape: u8) -> &'static [Vector2] {
    const I: &[Vector2] = &[
        Vector2::new(0.0, 0.0),
        Vector2::new(4.0, 0.0),
        Vector2::new(4.0, 1.0),
        Vector2::new(0.0, 1.0),
    ];
    const O: &[Vector2] = &[
        Vector2::new(0.0, 0.0),
        Vector2::new(2.0, 0.0),
        Vector2::new(2.0, 2.0),
        Vector2::new(0.0, 2.0),
    ];
    const T: &[Vector2] = &[
        Vector2::new(0.0, 0.0),
        Vector2::new(3.0, 0.0),
        Vector2::new(3.0, 1.0),
        Vector2::new(2.0, 1.0),
        Vector2::new(2.0, 2.0),
        Vector2::new(1.0, 2.0),
        Vector2::new(1.0, 1.0),
        Vector2::new(0.0, 1.0),
    ];
    const L: &[Vector2] = &[
        Vector2::new(0.0, 0.0),
        Vector2::new(1.0, 0.0),
        Vector2::new(1.0, 2.0),
        Vector2::new(2.0, 2.0),
        Vector2::new(2.0, 3.0),
        Vector2::new(0.0, 3.0),
    ];
    const S: &[Vector2] = &[
        Vector2::new(1.0, 0.0),
        Vector2::new(3.0, 0.0),
        Vector2::new(3.0, 1.0),
        Vector2::new(2.0, 1.0),
        Vector2::new(2.0, 2.0),
        Vector2::new(0.0, 2.0),
        Vector2::new(0.0, 1.0),
        Vector2::new(1.0, 1.0),
    ];
    match shape % TETROMINO_SHAPES {
        0 => I,
        1 => O,
        2 => T,
        3 => L,
        _ => S,
    }
}

/// Build a finalized tetromino ink body centered on `at`, already Traveling (`vel` px/FRAME —
/// spawned finalized + moving is the plan's "fired ink" lifecycle: it arcs, stacks/settles, and
/// then lives exactly like drawn ink: standable terrain, strikeable one-bone body, persistable.
// parity(v1-tetromino-ink-lifecycle): a fired or dropped piece is born as traveling permanent ink, then stacks, settles, reclassifies, supports, takes damage, and can unlock again
pub fn tetromino_path(
    shape: u8,
    at: Vector2,
    vel: Vector2,
    props: StrokeProps,
    owner: i8,
    tick: u64,
    nodes: &mut [InkNode],
    free: &mut FreeSpans,
) -> InkPath {
    let outline = tetromino_outline(shape);
    let center = outline.iter().copied().fold(Vector2::ZERO, |a, b| a + b) / outline.len() as f32;
    let mut p = InkPath::EMPTY;
    p.owner = owner;
    p.drawing = true; // local == world while pushing, like a live draw
    p.props = props;
    // reserve a full span up front (the drawing convention); `finalize_path` shrinks it to fit.
    p.start = free
        .alloc(MAX_PATH_PTS as u16)
        .expect("tetromino span fits the pool");
    for v in outline {
        p.push(at + (*v - center) * TETRIS_CELL, tick, nodes);
    }
    p.push(at + (outline[0] - center) * TETRIS_CELL, tick, nodes); // close the loop
    finalize_path(&mut p, nodes, free);
    p.vel = vel;
    p
}

/// Kill still player ink whose reference point settled outside the blast zone (reading A: the zone
/// is the weapon — knock enemy ink out, it dies where it lands). Traveling ink is exempt (it gets
/// judged when it settles); baked stage (mass 0 / owner < 0) is exempt always. With no zone ink
/// down yet, the static fighter blast frame is the boundary.
// parity(v1-ink-zone-prune): settled player ink outside the static-or-extended live zone is released while traveling and baked strokes remain exempt
pub(crate) fn prune_outside(n: &mut SimState) {
    // Static frame UNIONED with the live zone-ink bounding box, mirroring `ZoneRect::extended`: zone
    // ink (which now always includes the zone-material hull) only ever GROWS the survivable
    // rect. Pre-hull this fn REPLACED the rect with the zone bounding box -- with the hull counted,
    // that shrank the whole world to the ship's own box and pruned every settled stroke on
    // the stage (2026-07-04: dropped tetris pieces vanished on landing).
    let stat_lo = Vector2::new(BLAST_LEFT, BLAST_TOP);
    let stat_hi = Vector2::new(BLAST_RIGHT, BLAST_Y);
    let (lo, hi) = match ink_blast_zone(&n.paths, &n.nodes) {
        None => (stat_lo, stat_hi),
        Some((zlo, zhi)) => (stat_lo.min(zlo), stat_hi.max(zhi)),
    };
    for slot in 0..MAX_DRAWN {
        let p = &n.paths[slot];
        if !p.active() || p.owner < 0 || p.mass <= 0.0 || p.traveling() || p.drawing {
            continue;
        }
        let c = p.pos;
        if c.x < lo.x || c.x > hi.x || c.y < lo.y || c.y > hi.y {
            n.paths[slot].release(&mut n.free);
        }
    }
}

// ── striking ink (billiards: anything moving can knock ink) ──────────────────────────────────────

/// Ink mass → the kb formula's weight param. A ~300px default-density stroke weighs in near a
/// fighter (Tune.weight ~104); longer/denser strokes are heavier and fly less far.
pub const INK_WEIGHT_SCALE: f32 = 0.35;

pub fn mass_as_weight(mass: f32) -> f32 {
    mass * INK_WEIGHT_SCALE
}

/// A hit landing on a finalized ink body: the un-lock primitive (plans/ink-billiards.md step 4),
/// now a thin wrapper over `combat::strike` — the formula half lives in the shared ritual, the
/// launch-vs-shake policy in `InkPath::absorb`. `dmg` is passed separately so callers keep their
/// own scaling (auto-fire weakness, blast falloff).
// parity(v1-ink-strike-unlock): segment contact adds damage and either shakes a locked stroke or launches and spins the whole body from the hit point
pub fn resolve_hit_ink(
    hb: &Hitbox,
    dmg: f32,
    facing: f32,
    contact: Vector2,
    ink: &mut InkPath,
    t: &Tune,
) {
    strike(
        ink,
        &Swing {
            hb,
            dmg,
            kb_scale: 1.0,
            hitlag_bonus: 2,
            interrupt: false,
            aim: Aim::Angle {
                deg: hb.angle,
                facing,
            },
        },
        contact,
        t,
    );
}

/// Ink is another Sandbag (plans/body-bus.md): hurt shapes + damage% + weight into the shared
/// KB formula, no FSM, never promoted to fighter. The launch-vs-shake threshold — a weak graze
/// can't knock a platform out from under someone, but chip damage still builds `percent` toward a
/// real launch — is THIS impl's policy, living in `absorb`, not bus law.
impl PunchableFace for InkPath {
    fn bound(&self) -> (Vector2, f32) {
        // `strike` reads only `.0` (the aim target = `pos`); the broadphase RADIUS an ink body
        // needs comes from `bound_circle(nodes)` at the geometry-aware call sites (`strike_ink`,
        // `ink_hits_fighters`), which read the arena directly. The trait can't thread `nodes`
        // (Fighter shares it), so this returns the pos with a 0 radius -- never used as a radius.
        (self.pos, 0.0)
    }
    fn hurt_shapes(&self, _out: &mut impl FnMut(geo::Iso, geo::Shape)) {
        // Ink's narrowphase does NOT flow through the trait (it needs the node pool, which the
        // trait can't carry): `strike_ink`/`ink_hits_fighters` walk the polyline segments off the
        // arena themselves. This impl exists only to satisfy `PunchableFace`; it emits nothing and
        // is never called for ink today.
    }
    fn percent(&self) -> f32 {
        self.percent
    }
    fn heft(&self, _t: &Tune) -> f32 {
        mass_as_weight(self.mass)
    }
    fn guard(&self) -> Guard {
        if self.mass <= 0.0 || self.drawing {
            Guard::Invuln // baked stage / still-being-authored ink is not a body
        } else {
            Guard::Open
        }
    }
    fn absorb(&mut self, l: crate::v1::combat::Launch, contact: Vector2, t: &Tune) {
        self.percent += l.dmg;
        if let Some(cell) = self.cell {
            // Cell attachment is authored durability policy. Intact cells retain their motion;
            // depleted cells inherit the ordinary strike launch when converted into an item.
            if self.percent >= cell.durability {
                self.vel = l.vel * DT;
            } else {
                self.shake = (l.dmg * HITLAG_PER_DMG) as i64 + 2;
            }
            return;
        }
        if l.speed >= t.ink_launch_speed {
            // ink vel is px/frame (integrate_ink adds gravity and steps pos += vel with no DT).
            self.vel = l.vel * DT;
            let r = contact - self.pos;
            let rr = r.length_squared();
            if rr > 1.0 {
                // spin as if the launch impulse acted at the contact: ω matches the tangential
                // rate there — clipping a corner sends it tumbling, the Brawl soccer ball.
                self.omega =
                    ((r.x * self.vel.y - r.y * self.vel.x) / rr).clamp(-INK_MAX_SPIN, INK_MAX_SPIN);
            }
            self.shake = 0; // launched: the travel IS the reaction
        } else {
            self.shake = (l.dmg * HITLAG_PER_DMG) as i64 + 2; // too weak to un-lock: jiggle in place
        }
    }
}

/// Point-contact strike: apply `hb` to every finalized ink body within `r` of point `p` (coarse
/// bound-circle cull, then per-segment capsule test). Returns true if anything was struck so a
/// projectile can spend itself. `facing` signs the launch direction exactly like a fighter hit.
pub(crate) fn strike_ink(
    paths: &mut [InkPath; MAX_DRAWN],
    p: Vector2,
    r: f32,
    hb: &Hitbox,
    dmg: f32,
    facing: f32,
    nodes: &[InkNode],
    t: &Tune,
) -> bool {
    let mut hit = false;
    for ink in paths.iter_mut() {
        if !ink.active() || ink.mass <= 0.0 || ink.drawing {
            continue;
        }
        let (c, br) = ink.bound_circle(nodes);
        if !geo::circles_touch(p, r, c, br) {
            continue;
        }
        let n = ink.len as usize;
        // contact = the closest segment point inside the strike circle (feeds the launch torque)
        let touched = (0..n.saturating_sub(1)).find_map(|s| {
            let (a, b) = ink.world_seg(s, nodes);
            let q = geo::closest_on_seg(p, a, b);
            ((p - q).length() <= r).then_some(q)
        });
        if let Some(contact) = touched {
            resolve_hit_ink(hb, dmg, facing, contact, ink, t);
            hit = true;
        }
    }
    hit
}

// ── persistence helpers (A4: permanent ink ↔ durable world) ──────────────────────────────────────

/// Ramer-Douglas-Peucker polyline simplification: keeps both endpoints and every vertex whose
/// perpendicular distance from the chord of its span exceeds `eps` px. This is the persisted-ink
/// space saver: store only the bends — the straight segment between two kept vertices IS the
/// interpolation, so a hand-drawn wobble collapses to a few points instead of one per sample.
/// Deterministic, iterative (explicit stack), order-preserving.
// parity(v1-ink-persistence): durable strokes simplify in order and rehydrate through the same material, finalize, mass, and classification path as live ink
pub fn simplify_polyline(pts: &[Vector2], eps: f32) -> Vec<Vector2> {
    if pts.len() <= 2 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut spans = vec![(0usize, pts.len() - 1)];
    while let Some((a, b)) = spans.pop() {
        if b <= a + 1 {
            continue;
        }
        // farthest interior vertex from the chord a→b
        let (mut worst, mut worst_d) = (a, -1.0f32);
        for i in a + 1..b {
            let d = (pts[i] - geo::closest_on_seg(pts[i], pts[a], pts[b])).length();
            if d > worst_d {
                worst = i;
                worst_d = d;
            }
        }
        if worst_d > eps {
            keep[worst] = true;
            spans.push((a, worst));
            spans.push((worst, b));
        }
    }
    pts.iter()
        .zip(&keep)
        .filter(|(_, k)| **k)
        .map(|(p, _)| *p)
        .collect()
}

/// Build a live sim path from a persisted stroke: world-space `pts` in, material off the registry,
/// finalized (rebased + classified + massed) so it lands as a Still body. `owner` is the LOCAL match
/// attribution slot (the durable `PlayerId` stays in the world log; the sim only knows handles).
pub fn rehydrate_stroke(
    pts: &[Vector2],
    stroke: StrokeId,
    owner: i8,
    nodes: &mut [InkNode],
    free: &mut FreeSpans,
    t: &Tune,
) -> InkPath {
    let mut p = InkPath::EMPTY;
    p.owner = owner;
    p.drawing = true; // keep pos ZERO while pushing world points (local == world), like a live draw
    p.props = t.strokes.get(stroke);
    p.start = free
        .alloc(MAX_PATH_PTS as u16)
        .expect("rehydrate span fits the pool");
    for (i, w) in pts.iter().take(MAX_PATH_PTS).enumerate() {
        p.push(*w, i as u64, nodes);
    }
    finalize_path(&mut p, nodes, free);
    p
}

/// Stop drawing a path and cache its per-segment surface classes (the grabbability the collision read
/// consumes). Called on button release or budget exhaustion. Also rebases the geometry: `pos` becomes
/// the node centroid and `pts` become offsets from it, leaving every `world_pt` identical up to f32
/// rounding (pure translation bookkeeping — it lets the whole path move later by writing `pos` alone).
// parity(v1-ink-path-finalization): finishing preserves world vertices while recentering local geometry, computes mass from length and density, classifies surfaces, and releases unused storage
fn finalize_path(p: &mut InkPath, nodes: &mut [InkNode], free: &mut FreeSpans) {
    let span_was = p.span_len(); // MAX_PATH_PTS while still drawing -- the reservation to shrink
    p.drawing = false;
    let n = p.len as usize;
    let start = p.start as usize;
    if n > 0 {
        let sum = (0..n)
            .map(|i| nodes[start + i].pt)
            .fold(Vector2::ZERO, |a, b| a + b);
        let c = sum / n as f32 + p.pos;
        let shift = p.pos - c;
        for i in 0..n {
            nodes[start + i].pt += shift;
        }
        p.pos = c;
    }
    // body mass: stroke length × material density. density 0 (baked stage presets) leaves
    // mass 0 = the not-a-body sentinel, so those strokes can never be knocked into Traveling.
    p.mass = span_length(p, nodes) * p.props.density;
    classify(p, nodes);
    // shrink the reservation to the real length: hand the unused tail back to the pool. This is the
    // whole arena win -- a finalized stroke holds only its `len`, not a `MAX_PATH_PTS` reservation.
    if span_was > n {
        free.free((start + n) as u16, (span_was - n) as u16);
    }
}

mod billiard; // ink-vs-ink billiard impulse solver (split off stage/mod.rs for R5 headroom)
pub(crate) use billiard::{resolve_ink_billiard, write_billiard};
pub(crate) mod board; // ink-board slot policy: free slot, else evict by age (permanents last)
pub mod props; // stroke materials: GateSide + StrokeProps + StrokeRegistry (split for the R5 ratchet)
pub use props::{GateSide, STROKE_SLOTS, StrokeId, StrokeProps, StrokeRegistry};
pub mod fixtures; // baked test fixtures: the wall pillar + the triangle-wave mover
pub use fixtures::{
    MOVER_AMP, MOVER_HOME, MOVER_PERIOD, MOVER_SLOT, MOVER_W, PILLAR_BOT, PILLAR_SLOT, PILLAR_TOP,
    PILLAR_X, bake_mover, bake_pillar, mover_pos, mover_step,
};
pub mod bake_stage; // the baked main-stage builder (owner<-1 InkPaths); keystone step 1a, INERT
pub use bake_stage::{STAGE_STROKES, stage_strokes};

#[cfg(test)]
mod tests;
