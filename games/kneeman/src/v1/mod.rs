// Sealed legacy tree: kept for feel reference, never extended. Silenced wholesale rather than
// pruned warning-by-warning so real signal elsewhere in the workspace stays visible.
#![allow(dead_code, unused_imports)]

// Engine-agnostic vectors. `Vector2` is kept as the local name (minimal churn from the
// godot original); the shell converts to godot::Vector2 at the render boundary.
pub use glam::Vec2 as Vector2;

use serde::{Deserialize, Serialize};

/// Debug-only stderr log: the project's standard sim-side logging. Compiles to a NO-OP in release
/// builds (no perf cost, no stderr spam on web/release). In debug builds it writes one line to
/// stderr; the shell's `just run-debug` redirects stderr to `/tmp/smash_run.log`, so a repro
/// leaves a frame-by-frame trace filterable with `grep "[channel]"` (channels today: `[floor-catch]`,
/// `[floor-miss]`, `[wall-collide]`, `[ink-collide]`, emitted from `body/`). No dependency, no setup,
/// no global state -- one macro, gated on `cfg!(debug_assertions)`.
#[cfg(debug_assertions)]
#[macro_export]
macro_rules! sim_log {
    ($($arg:tt)*) => { eprintln!($($arg)*) };
}
#[cfg(not(debug_assertions))]
#[macro_export]
macro_rules! sim_log {
    ($($arg:tt)*) => {};
}

pub mod state; // ground-truth enums: CharState/Act/ThrowDir + the input-buffer Action/Lane/Slot
pub use state::{Act, Action, CharState, Slot, ThrowDir};
pub(crate) use state::{Lane, airborne};
pub mod fighter; // one fighter as a plain value: the Fighter struct + its impl, plus respawn
pub use fighter::Fighter;
pub(crate) use fighter::respawn;
pub mod fighters; // the fighter FSM as a Slice: FighterCx + FighterSlice (plans/core-rx-refactor.md section 3)
pub mod fx; // cosmetic event ring: FxKind/Fx + push_fx
pub use crate::input_frame::InputFrame;
pub(crate) use fx::push_fx;
pub use fx::{Fx, FxKind, MAX_FX}; // one frame of sampled input (shared shell type)
pub mod body; // the terrain bus: Surf soup + Randall + sweeps + the one refresh site (plans/body-bus.md)
pub mod combat; // one hit ritual: strike + PunchableFace (plans/body-bus.md)
pub use combat::*;
pub(crate) use combat::{ink_hits_fighters, resolve_clank, resolve_combat};
pub mod geo; // deterministic collision geometry, API-shaped to mirror parry2d (swap-in later)
pub mod mechanics; // hot-loadable closed mechanics language + generic vector-body content types
pub mod slice; // reducer trait + combinators: Slice/Never/Then/MapEffect (plans/step-slices.md)
pub mod stage;
pub mod step; // the step pipeline as a Then chain of ZST phases (plans/core-rx-refactor.md section 2) // all surfaces: static stage geometry + drawn ink paths
pub use stage::*;
pub mod arena; // shared-node pool + deterministic free-span allocator (plans/ink-storage-arena.md slice 4)
pub use arena::{FreeSpans, InkNode, NODE_POOL};
pub mod zone; // the live/toggleable blast zone: ZoneRect + zone-maker ink bounding box + item exemptions
pub use zone::*;
pub mod physics; // kinematics: DI, drift, dodge, ledge snap, math helpers
pub(crate) use physics::*; // all crate-internal helpers; nothing here is part of the public API
pub mod item; // items + projectiles: pickups, bolts/bombs, spawning, projectile resolution
pub use item::*;
mod rng; // the sim's one LCG step + its Zoom wiring onto SimState.rng (core-rx-refactor.md row 10)
pub(crate) use rng::roll_rng;
pub mod ac; // the Armored Core overlay: badge-gated boost frame + decoupled arm weapons
mod items; // per-kind item behavior (plans/mod-api.md Tier 0): sealed trait, one file per kind
pub use items::cards::{ItemCard, MENU_ITEMS};
pub mod moves; // moves by kind: shared attack data + special + throw
pub use moves::*;
pub mod tune; // character attributes (CharData) + derived pixel-space feel config (Tune)
pub use tune::*;
pub mod acts;
mod chars; // per-character CharSpec rows (plans/mod-api.md Tier 1): one file per character
pub mod tune_paths; // GENERATED (.dl/gen-tune-paths.dl): TunePath enum + lens ZSTs over Tune (core-rx-refactor.md row 10)
mod za_warudo; // the per-fighter state machine (reduce_next_state): freeze, re-derive, resume // actuate a fighter's emitted Act into the SimState: apply_act + footstool
pub(crate) use acts::apply_act;
pub mod ship; // the parked ship: helm steering (who's piloting) + the booster exhaust volume
pub mod station; // ship stations (plans/lovers-ship.md v2): mounted item -> lock a rider to an anchor
pub(crate) use ship::{booster_blast, steer_from_riders};
pub use station::{SHIP_STATIONS, nearest_station, spawn_station, station_anchor};

pub mod net;
pub mod world; // v2 mvp: the durable event-sourced world layer (types/fold/bridge/migrate; store gated) // folded former `smash_net` crate: ggrs rollback glue, wire input, lobby/assets (v1 is its only consumer)

// fixed timestep; the sim never uses wall-clock delta (determinism).
pub const DT: f32 = 1.0 / 60.0;
pub const FPS: f32 = 60.0;

// World->screen scale. Source attributes are world-units; we render pixels.
// Spatial FEEL (jump-height : run-distance ratios, time-to-apex) is scale-invariant,
// so this just sets how big the world reads on screen. Bumped to fit the larger stage.
// Change it and every distance scales together.
pub const PX_PER_UNIT: f32 = 7.0;

// Environment Collision Box: a diamond carried with the fighter, like classic platform fighters.
// `pos` is the BOTTOM vertex (the feet); the other three verts sit a half-height up and to the
// sides. The bottom vert lands on floors; the side verts collide with stage walls.
pub const ECB_HALF_W: f32 = 38.0; // left/right vert offset from center (x)
pub const ECB_HALF_H: f32 = 70.0; // top/bottom vert offset from center (y); body ~140px ≈ 3/4 the
// ground→side-platform gap (185px). Was 42 (jiggly-sized).

/// The four ECB verts in WORLD space for a feet position, ordered [top, right, bottom, left].
pub fn ecb_verts(feet: Vector2) -> [Vector2; 4] {
    let cy = feet.y - ECB_HALF_H; // diamond center y (bottom vert = feet)
    [
        Vector2::new(feet.x, cy - ECB_HALF_H), // top
        Vector2::new(feet.x + ECB_HALF_W, cy), // right
        feet,                                  // bottom (feet)
        Vector2::new(feet.x - ECB_HALF_W, cy), // left
    ]
}

pub const DUMMY_R: f32 = 48.0; // body hurtbox radius (circle), scaled with the taller ECB

/// Fresh shield health. `Fighter::spawn` can't see `Tune` (no param), so the field inits from
/// this const; `Tune.shield_max` defaults to the same value and the regen clamp uses the Tune.
pub const SHIELD_MAX: f32 = 60.0;

/// Max simultaneous items+projectiles on screen. Fixed-size (not a `Vec`): the rollback ring does a
/// raw memcpy of `SimState` and `Mutable::get()` requires `Copy`, so every array here must stay a
/// compile-time-sized field, not grow dynamically. 128 is "no limits" sized honestly, not literally
/// unbounded. See plans/body-bus.md step 10 for the shell-side pressure warn that fires well before
/// a match actually fills this.
pub const MAX_ITEMS: usize = 128;

/// Max fighters in one match. Fixed (not a `Vec`) so `SimState` stays `Copy` and rollback snapshots
/// don't heap-allocate; `SimState::active` says how many of the slots are actually in play. 4 is the
/// canonical platform-fighter cap. See plans/n-player.md.
pub const MAX_PLAYERS: usize = 4;

/// The entire sim state as a plain value: the fighters (slots `0..active`) + the item field. This is
/// what the BehaviorSubject holds, what ggrs saves/rolls back, and what egui renders. `Copy` so
/// snapshots are free.
#[derive(Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimState {
    pub fighters: [Fighter; MAX_PLAYERS],
    pub active: u8, // fighters[0..active] are live; the rest are dormant (not stepped, not drawn)
    // serde's hand-rolled array impls only go up to len 32 (MAX_ITEMS=128 is past that); BigArray
    // is the standard const-generic workaround, same wire shape (a fixed-length tuple) either way.
    #[serde(with = "serde_big_array::BigArray")]
    pub items: [Item; MAX_ITEMS],
    #[serde(with = "serde_big_array::BigArray")] // MAX_DRAWN=48 is past serde's len-32 impls too
    pub paths: [InkPath; MAX_DRAWN], // drawn ink + baked stage strokes; same polyline primitive
    pub tick: u64,        // global frame counter (drives item spawn cadence)
    pub rng: u64, // deterministic LCG state (item spawn positions/kinds; rolls back with state)
    pub helm: Helm, // the parked ship's engine: aim + throttle, steered by whoever rides the hull
    pub fx: [Fx; MAX_FX], // cosmetic event ring (explosions, muzzle flashes) — see `push_fx`
    pub fx_head: u8, // next fx slot to overwrite
    // ── shared ink-node arena (plans/ink-storage-arena.md slice 4) ──────────────────────────────
    // `InkPath` no longer carries its geometry inline; each path is a `{start,len}` handle into this
    // pool and `free` hands out spans of it. APPENDED at the END (positional bincode), so the only
    // wire break is `paths`' element type shrinking -- both peers run the same binary. Rolls back +
    // checksums like every other field.
    #[serde(with = "serde_big_array::BigArray")]
    // NODE_POOL=640 is past serde's len-32 array impls
    pub nodes: [InkNode; NODE_POOL],
    pub free: FreeSpans, // deterministic first-fit span allocator over `nodes`
}

impl SimState {
    /// Two fighters facing each other on the main stage (airborne drop-in). The default match size.
    pub fn spawn() -> Self {
        Self::spawn_n(2)
    }

    /// `count` fighters (clamped to `1..=MAX_PLAYERS`) dropped in evenly across the stage, each
    /// facing the stage centre. Dormant slots still hold a valid `Fighter` (so the array is sound)
    /// but `active` excludes them from stepping, combat, and rendering.
    // parity(v1-match-spawns): spawn_n uses spawn_slot to place active fighters facing stage center
    pub fn spawn_n(count: usize) -> Self {
        let count = count.clamp(1, MAX_PLAYERS);
        let mut fighters = [Fighter::spawn(480.0, 1.0); MAX_PLAYERS];
        for (p, f) in fighters.iter_mut().enumerate() {
            let (x, facing) = spawn_slot(p, count);
            *f = Fighter::spawn(x, facing);
            f.char_id = (p % 2) as u8; // default cast alternates (the shell's frog/zombie pair)
        }
        let mut nodes = [InkNode::ZERO; NODE_POOL];
        let mut free = FreeSpans::new();
        let mut paths = [InkPath::EMPTY; MAX_DRAWN];
        // parity(v1-stage-ink-fixture-bootstrap): match spawn allocates ship, pillar, and mover ink fixtures in a fixed rollback-stable order before installing the parked helm and station composition
        // Bake the three stage fixtures INTO the pool in a FIXED order (ship, pillar, mover), so the
        // spans they claim have identical `start` values on every peer -- the rollback invariant.
        paths[stage::SHIP_SLOT] = stage::bake_ship(&mut nodes, &mut free); // the parked hull, terrain from frame 0
        paths[stage::PILLAR_SLOT] = stage::bake_pillar(&mut nodes, &mut free); // walljump-on-ink wall fixture
        paths[stage::MOVER_SLOT] = stage::bake_mover(&mut nodes, &mut free); // the triangle-wave moving platform
        let mut state = Self {
            fighters,
            active: count as u8,
            items: [Item::EMPTY; MAX_ITEMS],
            paths,
            tick: 0,
            rng: 0x9E37_79B9_7F4A_7C15, // fixed seed: every peer spawns identical items
            helm: Helm::PARKED,
            fx: [Fx::EMPTY; MAX_FX],
            fx_head: 0,
            nodes,
            free,
        };
        // The helm station is a FIXTURE, seeded like the hull it's socketed to (2026-07-04
        // playtest: with no station in a live match the ship was unpilotable -- `spawn_station`
        // was only ever called from tests). Walk up + attack/grab occupies; c-stick aims,
        // attack thrusts (`ship::steer_from_riders`).
        station::spawn_station(&mut state, 0).expect("fresh item array always has a free slot");
        state
    }

    /// A LOCAL-ONLY debug playground (never a net/rollback-shared spawn): the ship plus one of
    /// every collision-surface kind, laid out so a player can walk a start pad, jump/drop onto each
    /// sample, and watch what lands / tunnels / falls / bounces. NOT a fix and NOT a test harness --
    /// it stages the jank so a human can drive it. Deterministic pure construction (fixed slot/alloc
    /// order, no RNG, no env reads), so if it ever rides a local rollback it re-derives identically.
    ///
    /// What is where, left to right in world space (y is screen-DOWN, GROUND_Y = 760):
    /// - the SHIP hull (`bake_ship`, canonical `SHIP_HOME` off-stage-left, x ~ -496..116): top rim to
    ///   land on, sloped dome shoulders (the historical tunneling lane the containment resolver
    ///   guards), and the hatch throat you drop straight into to stand inside the bowl.
    /// - a START PAD: the up-to-`MAX_PLAYERS` fighters parked grounded + idle at x ~ 200.. on the
    ///   static main floor, facing the sample row.
    /// - a long custom BASE FLOOR (solid Floor stroke, x 140..1520 @ GROUND_Y) to stand/walk and land
    ///   back onto after each sample.
    /// - an elevated SAMPLE ROW at y = 520 (jump-reachable), ~250px apart: a grabbable LEDGE (open
    ///   4-node polyline, tips classify Ledge), a SOFT platform (drop-through with down), a SOLID
    ///   drawn-style ink Floor, and a ONE-WAY ink gate (drawn R->L so its `PassForward` normal points
    ///   up -- land from above, pass from below).
    /// - the WALL pillar (`bake_pillar`, canonical x 980) between the solid-ink and one-way samples:
    ///   a vertical face to run/hop into (reflect test).
    /// - the moving platform (`bake_mover`, canonical upper-right triangle-wave) for a moving-surface
    ///   ride, reached by a big jump.
    ///
    /// The ambient one-arena static geometry (`PLATFORMS` + main-stage walls, emitted by
    /// `Soup::collect` regardless of `paths`) is unavoidable here -- runtime-selectable stages are a
    /// later task (see `StageSpec`) -- so the start pad rides the always-present main floor and the
    /// custom base floor overlaps/extends it; the deliberate SAMPLE ROW is the diagnostic.
    pub fn spawn_drop_test() -> Self {
        // Fixed elevated row height + baked-stroke slot assignments (low slots; the three baker
        // fixtures keep their reserved high slots so `drive_mover`/hull machinery still find them).
        const ROW_Y: f32 = 520.0;
        const BASE_SLOT: usize = 0; // long floor to stand on
        const LEDGE_SLOT: usize = 1; // grabbable ledge sample
        const SOFT_SLOT: usize = 2; // drop-through soft platform sample
        const SOLID_INK_SLOT: usize = 3; // solid drawn-style ink Floor sample
        const ONEWAY_SLOT: usize = 4; // one-way (PassForward) ink gate sample

        let mut fighters = [Fighter::spawn(200.0, 1.0); MAX_PLAYERS];
        for (idx, fighter) in fighters.iter_mut().enumerate() {
            let start_x = 200.0 + idx as f32 * 70.0; // start pad, left of the sample row
            *fighter = Fighter::spawn(start_x, 1.0);
            // grounded + idle on the always-present static main floor (PLATFORMS[0]); mirrors the
            // proven parked-fighter rig in drop_harness_tests (ground_plat = 0, ground_ink = -1).
            fighter.pos = Vector2::new(start_x, stage::GROUND_Y);
            fighter.vel = Vector2::ZERO;
            fighter.state = CharState::Stand;
            fighter.ground_plat = 0;
            fighter.ground_ink = -1;
            fighter.char_id = (idx % 2) as u8; // alternate the shell's frog/zombie pair for contrast
        }

        let mut nodes = [InkNode::ZERO; NODE_POOL];
        let mut free = FreeSpans::new();
        let mut paths = [InkPath::EMPTY; MAX_DRAWN];

        // Custom baked terrain strokes (owner < 0, density 0, immovable) in a FIXED alloc order for
        // determinism -- same bake_span + classify discipline as bake_pillar/bake_mover/stage_strokes.
        // 1. Long base floor to stand on (solid).
        paths[BASE_SLOT] = baked_terrain(
            &[
                Vector2::new(140.0, stage::GROUND_Y),
                Vector2::new(200.0, stage::GROUND_Y),
                Vector2::new(1460.0, stage::GROUND_Y),
                Vector2::new(1520.0, stage::GROUND_Y),
            ],
            true,
            GateSide::Off,
            &mut nodes,
            &mut free,
        );
        // 3. Grabbable Ledge: open 4-node polyline (tips classify Ledge, interior Floor), like the
        //    main-stage top edge in stage_strokes (24px tip). Solid so the tips are real lips.
        paths[LEDGE_SLOT] = baked_terrain(
            &[
                Vector2::new(300.0, ROW_Y),
                Vector2::new(324.0, ROW_Y),
                Vector2::new(456.0, ROW_Y),
                Vector2::new(480.0, ROW_Y),
            ],
            true,
            GateSide::Off,
            &mut nodes,
            &mut free,
        );
        // 4. Soft platform (drop-through): a 2-node soft stroke (solid = false) -- land from above,
        //    hold down to fall through.
        paths[SOFT_SLOT] = baked_terrain(
            &[Vector2::new(560.0, ROW_Y), Vector2::new(740.0, ROW_Y)],
            false,
            GateSide::Off,
            &mut nodes,
            &mut free,
        );
        // 5. Solid drawn-style ink Floor: a 2-node solid stroke to land on (baked owner < 0 solid).
        paths[SOLID_INK_SLOT] = baked_terrain(
            &[Vector2::new(820.0, ROW_Y), Vector2::new(960.0, ROW_Y)],
            true,
            GateSide::Off,
            &mut nodes,
            &mut free,
        );
        // 6. One-way ink gate (PassForward): drawn RIGHT->LEFT so the a->b tangent's +90 gate normal
        //    points UP -- a downward drop lands (blocked side), a rise from below passes through.
        paths[ONEWAY_SLOT] = baked_terrain(
            &[Vector2::new(1260.0, ROW_Y), Vector2::new(1080.0, ROW_Y)],
            false,
            GateSide::PassForward,
            &mut nodes,
            &mut free,
        );

        // The three canonical baker fixtures, in the same FIXED order + reserved slots as spawn_n so
        // their spans land identically and `drive_mover`/hull containment find them by slot.
        paths[stage::SHIP_SLOT] = stage::bake_ship(&mut nodes, &mut free); // 7. ship hull/container
        paths[stage::PILLAR_SLOT] = stage::bake_pillar(&mut nodes, &mut free); // 2. wall (reflect)
        paths[stage::MOVER_SLOT] = stage::bake_mover(&mut nodes, &mut free); // 8. moving platform

        Self {
            fighters,
            active: MAX_PLAYERS as u8,
            items: [Item::EMPTY; MAX_ITEMS], // no station/items: the surfaces are the diagnostic
            paths,
            tick: 0,
            rng: 0x9E37_79B9_7F4A_7C15, // fixed seed, same as spawn_n (no RNG is actually consumed here)
            helm: Helm::PARKED,
            fx: [Fx::EMPTY; MAX_FX],
            fx_head: 0,
            nodes,
            free,
        }
    }
}

/// Bake one immovable terrain stroke from world-space `world` points: PEN material, density 0
/// (mass 0 = immovable, strike/prune/zone-exempt), `owner = -1` (from `InkPath::EMPTY`, never
/// expires), `pos = ZERO` so the stored offsets ARE world coords. `bake_span` writes the span and
/// runs `classify`. Mirrors `bake_stage::terrain_stroke`, re-expressed at the crate root because
/// that one is private -- used only by `spawn_drop_test`.
fn baked_terrain(
    world: &[Vector2],
    solid: bool,
    gate: GateSide,
    nodes: &mut [InkNode],
    free: &mut FreeSpans,
) -> InkPath {
    let mut path = InkPath::EMPTY;
    path.props = StrokeProps::PEN;
    path.props.density = 0.0;
    path.props.solid = solid;
    path.props.gate_side = gate;
    stage::bake_span(&mut path, world, Vector2::ZERO, nodes, free);
    path
}

/// Bytes `state` occupies on the wire under the crate's default bincode config -- the same
/// zero-Options `bincode::serialize`/`serialized_size` calls `net`'s checksum/resume snapshot and
/// `world::canon` already use (bincode 1.x default is fixint LE, stable), not a second config.
/// Debug/UI only (the pause-menu world-size chip); never called from `step()`.
pub fn state_size_bytes(s: &SimState) -> u64 {
    bincode::serialized_size(s).expect("SimState always encodes")
}

/// Spawn position + facing for player `p` of `count`. Two players keep the historical 480/720 split
/// (so existing behavior/tests are byte-identical); more players spread evenly over the same band,
/// each turned toward stage centre.
fn spawn_slot(p: usize, count: usize) -> (f32, f32) {
    const CENTER: f32 = 600.0;
    if count <= 2 {
        return if p == 0 { (480.0, 1.0) } else { (720.0, -1.0) };
    }
    const LEFT: f32 = 360.0;
    const RIGHT: f32 = 840.0;
    let t = p as f32 / (count - 1) as f32;
    let x = LEFT + (RIGHT - LEFT) * t;
    (x, if x < CENTER { 1.0 } else { -1.0 })
}

/// PURE scan step: (state, input, tune) -> next state.
/// No engine calls, no IO, no &mut self. Deterministic given the same inputs.
/// `states = inputs.scan(SimState::spawn(), step)`.
/// One tick of the whole sim: advance each fighter from its own input, then resolve combat
/// both directions. Pure value-in/value-out — this is what ggrs calls (possibly N times per
/// frame during rollback). `inputs[k]` drives `fighters[k]`.
pub fn step(s: &SimState, inputs: &[&InputFrame], t: &Tune) -> SimState {
    // Recompute hoist (core-rx-refactor.md row 10 commit 3): `tunes` and `np` used to be
    // rebuilt as a phase-local in every phase that needed them (4x and 6x respectively,
    // see step.rs's module doc). Computed ONCE here, from the frame-start state `s`, before
    // any phase runs -- `char_id` and `active` are untouched by every phase this same tick
    // (`char_id` only changes at spawn/respawn, which preserves it), so this is bit-identical
    // to what each phase would have recomputed for itself. `tunes` lives once in THIS frame
    // (~19KB per `Tune` x MAX_PLAYERS) and rides the pipeline as a `&'_` on `StepCx`, not by
    // value -- see StepCx's doc for why (a by-value copy through the `Then` chain's nested
    // calls overflowed the test-thread stack in an unoptimized build).
    let tunes: [Tune; MAX_PLAYERS] = core::array::from_fn(|p| t.for_char(s.fighters[p].char_id));
    let np = (s.active as usize).min(inputs.len());
    let mut n = *s;
    // The pipeline is now `type Step = (Tick, Fsm, Actuate, StrikeInk, Combat, Items, Ink)`
    // (core/src/step.rs): order is a type, purity is `Effect = Never`. `step()` keeps its exact
    // public signature so ggrs and every test are untouched. `Never` makes the sink unreachable.
    <step::Step as slice::Slice>::reduce(
        &mut n,
        (),
        step::StepCx {
            inputs,
            tune: t,
            tunes: &tunes,
            np,
        },
        &mut |never: slice::Never| match never {},
    );
    n
}

/// Two distinct fighters (`a != b`) borrowed mutably at once, via one `split_at_mut`. Replaces the
/// old hand-rolled `split_at_mut(1)` now that the pair is dynamic.
pub(crate) fn pair_mut(fs: &mut [Fighter], a: usize, b: usize) -> (&mut Fighter, &mut Fighter) {
    debug_assert_ne!(a, b, "pair_mut needs two distinct fighters");
    if a < b {
        let (l, r) = fs.split_at_mut(b);
        (&mut l[a], &mut r[0])
    } else {
        let (l, r) = fs.split_at_mut(a);
        (&mut r[0], &mut l[b])
    }
}

#[cfg(test)]
mod ac_tests;
#[cfg(test)]
mod art_pass_tests;
#[cfg(test)]
mod charspec_tests;
#[cfg(test)]
mod cmd_grab_tests;
#[cfg(test)]
mod di_tests;
#[cfg(test)]
mod foundations_tests;
#[cfg(test)]
mod geo_wiring_tests;
#[cfg(test)]
mod grab_tests;
#[cfg(test)]
mod hull_gate_tests;
#[cfg(test)]
mod item_hurt_tests;
#[cfg(test)]
mod items_land_tests;
#[cfg(test)]
mod land_cancel_tests;
#[cfg(test)]
mod ledge_ink_tests;
#[cfg(test)]
mod mover_tests;
#[cfg(test)]
mod rider_follow_tests;
#[cfg(test)]
mod ship_contain_tests;
#[cfg(test)]
mod ship_tests;
#[cfg(test)]
mod surf_vel_tests;
#[cfg(test)]
mod teleport_tests;
#[cfg(test)]
mod tetris_drop_tests;
#[cfg(test)]
mod wall_corner_tests;
#[cfg(test)]
mod wings_wear_tests;
#[cfg(test)]
mod zone_tests;

#[cfg(test)]
mod state_budget_tests;
// Former `core-v1-legacy` integration tests (crate-root `tests/*.rs`): folded in as ordinary
// test modules alongside the unit tests above -- an external `tests/` dir can only see PUBLIC
// API, and `v1` is deliberately no longer public outside this crate, so black-box coverage
// moves in-tree. `use crate::v1::*` (unchanged from their old `use smash_core::*`) still gives
// each one the same black-box view of the sim's public surface.
#[cfg(test)]
mod mechanics_document_tests; // was tests/mechanics_document.rs
#[cfg(test)]
mod replay_tests; // was tests/sim.rs: input builders + the deterministic replay harness
#[cfg(test)]
mod world_formula_e2e_tests; // was tests/world_formula_e2e.rs; `#![cfg(feature = "storage")]` internally

#[cfg(test)]
mod drop_test_scenario_tests {
    use super::{CharData, Fighter, InputFrame, MAX_PLAYERS, SimState, Tune};
    use crate::v1::CharState;
    use crate::v1::stage::SHIP_SLOT;

    const IDLE: InputFrame = InputFrame {
        dir: 0.0,
        aim_y: 0.0,
        cx: 0.0,
        cy: 0.0,
        jump: false,
        jump_held: false,
        shorthop: false,
        shield_held: false,
        shield_pressed: false,
        down: false,
        down_pressed: false,
        attack: false,
        attack_held: false,
        grab: false,
        special: false,
    };

    /// The playground constructs without panic, seats the ship, and stages exactly one of every
    /// surface kind (8 baked owner < 0 strokes: base floor, ledge, soft, solid ink, one-way, plus
    /// the ship / pillar / mover fixtures), with the fighters parked grounded + idle. Stepping a few
    /// frames confirms the strokes survive the real Soup/step pipeline without a panic.
    #[test]
    fn spawn_drop_test_constructs() {
        let mut state = SimState::spawn_drop_test();

        assert!(state.paths[SHIP_SLOT].active(), "the ship hull is present");
        let surfaces = state
            .paths
            .iter()
            .filter(|path| path.active() && path.owner < 0)
            .count();
        assert_eq!(
            surfaces, 8,
            "one of every surface kind: 5 custom + ship + pillar + mover"
        );

        assert_eq!(
            state.active as usize, MAX_PLAYERS,
            "all start-pad fighters live"
        );
        for fighter in &state.fighters[..MAX_PLAYERS] {
            assert_eq!(fighter.state, CharState::Stand, "parked grounded + idle");
            assert_eq!(fighter.ground_plat, 0, "grounded on the static main floor");
        }

        // Real pipeline smoke: a few IDLE steps must not panic and must leave the fighters grounded.
        let tune = Tune::from_char(&CharData::KNEEMAN);
        let inputs: [&InputFrame; MAX_PLAYERS] = [&IDLE; MAX_PLAYERS];
        for _ in 0..5 {
            state = super::step(&state, &inputs, &tune);
        }
        assert!(
            state.fighters[..MAX_PLAYERS].iter().all(Fighter::grounded),
            "start-pad fighters stay grounded through idle steps",
        );
    }
}
