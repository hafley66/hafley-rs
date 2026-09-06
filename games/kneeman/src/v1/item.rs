//! Items + projectiles: pickups (guns, pen), the things they fire (bolts, bombs), spawning, the
//! held-tool follow, and projectile resolution (bolt hits, bomb blast). `Item` is the single home
//! for the slot type carried in `SimState.items`. Pure; re-exported at the crate root.

use crate::v1::acts::{apply_item_act, attach_badge};
use crate::v1::body::{FloorHit, Soup, sweep_floors};
use crate::v1::combat::{Aim, blast_falloff};
use crate::v1::items::act::{ItemAct, ItemActs, StrikeFollow};
use crate::v1::items::{behavior, floor};
use crate::v1::zone::item_zone_exempt;
use crate::v1::{
    CharState, DT, FLOOR_LEFT, FLOOR_RIGHT, Fighter, FxKind, GROUND_Y, Hitbox, MAX_ITEMS,
    MAX_PLAYERS, SimState, StrokeId, TETRIS_CELL, TETROMINO_SHAPES, ThrowDir, ToolKind, Tune,
    Vector2, airborne, geo, hurtbox, out_of_bounds, push_fx, resolve_hit_ink, roll_rng, strike_ink,
    tetromino_path,
};
use serde::{Deserialize, Serialize};

/// Per-item-kind config (spawn rate + behavior + model). Lives in Tune so the panel edits it live.
/// `hit` reuses AttackData for the projectile's damage/knockback (startup/active/recovery unused).
#[derive(Copy, Clone, Serialize, Deserialize)]
pub struct ItemConfig {
    pub spawn_weight: f32, // relative spawn chance vs other kinds (0 = never spawns)
    pub ammo: i64,         // shots a fresh gun carries
    pub cooldown: i64,     // frames between shots (a clean tap)
    pub autofire_cd: i64,  // frames between shots while holding (shorter = drains faster)
    pub autofire_dmg: f32, // damage multiplier for held auto-fire bolts (< 1 = weaker)
    pub speed: f32,        // projectile speed (px/s)
    pub range: i64, // projectile lifetime in frames before it fizzles (for the bomb = its fuse)
    pub proj_gravity: f32, // px/s^2 pulling the projectile down (0 = straight laser; >0 = arcing lob)
    pub blast_r: f32,      // explosion radius on detonation (0 = single-target bolt, no AoE)
    pub model_id: u8,      // shell sprite key (rendering only; sim ignores it)
    pub hit: Hitbox,       // projectile / explosion damage + knockback (one box; transcendent)
    pub zone_exempt: bool, // this kind's items are never quiet-despawned by the blast zone
}

impl ItemConfig {
    pub const LASER: Self = Self {
        spawn_weight: 1.0,
        ammo: 16,
        cooldown: 6,       // ~10 shots/sec on clean taps
        autofire_cd: 4,    // ~15 shots/sec while held — drains the mag faster
        autofire_dmg: 0.6, // held spray is weaker per bolt (the funny tax)
        speed: 1400.0,
        range: 70,
        proj_gravity: 0.0, // dead-straight
        blast_r: 0.0,      // single-target
        model_id: 0,
        hit: Hitbox {
            r: 12.0,
            damage: 2.5,
            angle: 12.0, // near-flat: lasers push, don't launch
            bkb: 10.0,
            kbg: 18.0,
            transcendent: true,
            ..Hitbox::NONE
        },
        zone_exempt: false,
    };

    /// The AC energy cannon's round (and any future cannon's): slow, fat, hits like a truck.
    /// Cadence/speed/lifetime live in `ac::arm_spec` (the trigger); this row is the ROUND —
    /// spawn_weight/ammo/cooldown unused until a hand-held cannon item exists.
    pub const PLASMA: Self = Self {
        spawn_weight: 0.0,
        ammo: 0,
        cooldown: 0,
        autofire_cd: 0,
        autofire_dmg: 1.0,
        speed: 380.0,
        range: 90,
        proj_gravity: 0.0,
        blast_r: 0.0,
        model_id: 0,
        hit: Hitbox {
            r: 30.0,
            damage: 14.0,
            angle: 42.0,
            bkb: 46.0,
            kbg: 80.0,
            transcendent: true,
            ..Hitbox::NONE
        },
        zone_exempt: false,
    };

    /// Tetris gun: lobs a whole closed tetromino INK BODY per shot (no Item projectile — the piece
    /// claims a path slot and is the plan's "fired ink": Traveling from birth, stacks on still ink,
    /// standable + strikeable the moment it locks). `speed` is the lob px/s; `hit` unused for now
    /// (piece-vs-fighter impact is the traveling-ink→fighter follow-up).
    pub const TETRIS: Self = Self {
        spawn_weight: 0.6,
        ammo: 8,      // eight pieces per gun
        cooldown: 30, // deliberate lob, ~2/sec
        autofire_cd: 30,
        autofire_dmg: 1.0,
        speed: 950.0,      // lobbed up-and-forward; ink gravity arcs it down
        range: 0,          // unused: the piece is ink, not a timed projectile
        proj_gravity: 0.0, // unused: integrate_ink owns the arc
        blast_r: 0.0,
        model_id: 2,
        hit: Hitbox::NONE,
        zone_exempt: false,
    };

    /// Red gun: low ammo, lobs a slow arcing bomb that detonates on contact or fuse and blasts
    /// everyone nearby (the funny "shoot it at your homies" weapon). Big radial knockback = a kill.
    pub const BOMB: Self = Self {
        spawn_weight: 0.7, // a bit rarer than the laser
        ammo: 4,           // four lobs and the gun is spent
        cooldown: 28,      // deliberate, ~2 shots/sec; no real autofire
        autofire_cd: 28,
        autofire_dmg: 1.0, // no auto-fire weakness; every lob is full power
        speed: 900.0,      // lobbed forward, gravity drags it into an arc
        range: 110,        // ~1.8s fuse if it never touches anyone
        proj_gravity: 2400.0,
        blast_r: 170.0, // generous splash
        model_id: 1,    // red model key (shell)
        hit: Hitbox {
            r: 22.0, // contact radius of the bomb body
            damage: 16.0,
            angle: 55.0, // up-and-out pop
            bkb: 28.0,
            kbg: 88.0,
            transcendent: true,
            ..Hitbox::NONE // launches hard -> kills at mid %
        },
        zone_exempt: false,
    };
}

/// Directional item-throw config (Smash-style): the launch speed per stick direction plus the
/// `hit` the armed item deals to a non-thrower it touches. Lives in Tune so the panel edits it live.
/// Knockback runs through the same `knockback_units` formula as a fighter's hitboxes, so the box's
/// `bkb`/`kbg`/`angle` give "any amount of knockback" the user wants.
#[derive(Copy, Clone, Serialize, Deserialize)]
pub struct ThrowItem {
    pub fwd_speed: f32,  // px/s launched forward (toward facing)
    pub back_speed: f32, // px/s launched behind
    pub up_speed: f32,   // px/s launched up
    pub down_speed: f32, // px/s launched down (a spike toss)
    pub hit: Hitbox,     // damage + knockback the flying item deals on contact (transcendent)
}

impl ThrowItem {
    /// Strong default: a fast toss that launches hard. Panel-editable per field.
    pub const DEFAULT: Self = Self {
        fwd_speed: 1500.0,
        back_speed: 1200.0,
        up_speed: 2400.0, // fights gravity the whole way up: needs the headroom to read as a move
        down_speed: 1700.0,
        hit: Hitbox {
            r: 34.0,
            damage: 9.0,
            angle: 45.0, // up-and-out pop
            bkb: 42.0,
            kbg: 92.0,
            transcendent: true,
            ..Hitbox::NONE // launches hard = a kill at mid %
        },
    };
}

/// What an item slot is. `None` = empty slot. Add kinds freely; behavior dispatches by `match`
/// (the "trait methods" are functions keyed on kind), config lives per-kind in Tune.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ItemKind {
    None,
    LaserGun,  // pickup weapon: hold + attack to fire LaserBolts until ammo runs out
    LaserBolt, // the projectile a LaserGun fires
    BobGun,    // red pickup weapon: lobs an arcing explosive (Bob-omb-ish) per shot
    Bomb, // the arcing explosive a BobGun fires; detonates on contact or fuse with radial knockback
    Pen,  // drawing tool: hold + attack to lay down an ink path (the tool is in `Item.tool`)
    TetrisGun, // lobs a whole closed tetromino ink body — instantly standable, strikeable terrain
    InkGun, // drawn-shot gun: c-stick aims the anchor, hold A to draw a shape riding the hand,
    // release to fire it as an impulsed ink body (plans/body-bus.md step 8)
    WingsBadge, // badge: attaches on pickup (no hand slot, no drop/throw) — infinite air jumps
    AcCore, // AUTO-attaches on touch: body replaced by an Armored Core (boost + c-stick arm gun)
    Rocket, // straight-flying explosive (a bazooka round); detonates like a Bomb (shared blast)
    PlasmaBall, // slow fat energy shot (the AC cannon's round; any future cannon fires it too)
    TetrisDropper, // TetrisGun's sibling: same permanent TETRIS-row piece, pure vertical drop
    // spawned in front of the fighter instead of an arc lob (items/tetris_drop.rs)
    Station, // a mounted ship station (crate::v1::station): socketed to a `SHIP_STATIONS` anchor via
             // `Item.mount`, inert as an item; interacting LOCKS the fighter (`Fighter.station`)
}

/// A passive character mod, granted by picking up a badge item. Bitmask bits in
/// `Fighter.badges`: attaching is `|=`, checking is `has_badge`. A badge never occupies the
/// hand, so it is structurally undropable and unthrowable; respawns keep it (it belongs to
/// the player now, not the stock).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Badge {
    Wings = 1 << 0, // air jumps never run out
    /// Body replaced by an Armored Core: extreme gravity + held-jump boost thrust, jump-tap
    /// quick boost on a cooldown, c-stick fires the rolled arm weapon (`Fighter.arm`).
    /// Attach is touch-based (see the AcCore arm in `update_items`), never a button.
    AcCore = 1 << 1,
}

/// How many bits `Badge` can address (it's a `u8` bitmask): sizes `Fighter.badge_gas`, the
/// per-badge wear table (`fighters::wear`), so a future badge slots into the default de-spawn
/// timer without resizing anything.
pub const N_BADGE_BITS: usize = 8;

impl ItemKind {
    /// Projectiles are transient hit-effects, not pickups: they don't count toward the field's
    /// pickup cap and can never be grabbed. Derives from `behavior::spec_for` (mod-api.md Tier 0):
    /// every new kind's file sets this once in its `spec()`, not in a parallel match here.
    pub fn is_projectile(self) -> bool {
        behavior::spec_for(self).is_projectile
    }

    /// A held weapon that fires on the attack button. Guns count toward the one-pickup cap.
    /// Derives from `spec()`: hand-attached, aims the c-stick, doesn't draw ink.
    pub fn is_gun(self) -> bool {
        let s = behavior::spec_for(self);
        matches!(s.attach, behavior::Attach::Hand) && s.aims && !s.draws
    }

    /// A held drawing tool: attack lays ink instead of firing. Counts toward the pickup cap, follows
    /// the hand, and drops like a gun. Derives from `spec()`: hand-attached, draws, doesn't aim.
    pub fn is_pen(self) -> bool {
        let s = behavior::spec_for(self);
        matches!(s.attach, behavior::Attach::Hand) && s.draws && !s.aims
    }

    /// Held in hand on pickup (gun or pen): follows the hand, drops on grab/death.
    /// Derives from `spec()`: every `Attach::Hand` kind.
    pub fn is_held_tool(self) -> bool {
        matches!(behavior::spec_for(self).attach, behavior::Attach::Hand)
    }

    /// The badge this kind grants on pickup, if it IS a badge. Badge pickups consume the
    /// item and set the fighter's bit instead of taking the hand slot. Derives from `spec()`.
    pub fn badge(self) -> Option<Badge> {
        match behavior::spec_for(self).attach {
            behavior::Attach::Badge(b) => Some(b),
            _ => None,
        }
    }

    /// An idle, empty one unloads off the ground (a spent pen/ink-gun), unlike a spent gun
    /// which vanishes the instant it empties in `fire_gun`. Derives from `spec()`.
    pub(crate) fn despawn_when_spent(self) -> bool {
        behavior::spec_for(self).despawn_when_spent
    }
}

/// One item OR projectile. Plain Copy data so it rolls back. `owner`: -1 = unowned ground item;
/// else the fighter index that holds it (gun) or fired it (bolt). `timer`: gun = fire cooldown,
/// `gas` is the item's first-dimension use measure (float, covers every kind): gun shots left, a
/// pen's remaining ink length, a bolt's weak-flag. `gas_max` is the spawn value, so a HUD can show
/// gas/gas_max as 0..1. `timer` is the projectile lifetime / gun cooldown.
#[derive(Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub kind: ItemKind,
    pub pos: Vector2,
    pub vel: Vector2,
    pub owner: i8,
    pub gas: f32,
    pub gas_max: f32,
    pub timer: i64,
    pub facing: f32,
    pub tool: ToolKind, // which drawing tool, when `kind == Pen` (ignored otherwise)
    pub stroke: StrokeId, // which StrokeRegistry preset this pen stamps (row 0 = default)
    pub thrown: bool, // armed in flight from a directional throw: deals `throw_item.hit` to non-throwers.
    // While thrown, `owner` still holds the thrower idx (skips self); disarms to a
    // normal unowned ground item (owner -1, thrown false) once it settles.
    /// Ship-station mount (plans/lovers-ship.md "v2: stations", the `crate::v1::station` seam): the
    /// `SHIP_STATIONS` anchor this item is socketed to, or -1 = a free item. A mounted item is an inert
    /// fixture (never falls/moves/wears/despawns — `update_items` skips it) and is not pocketable;
    /// interacting OCCUPIES the station. Appended at the struct's END (bincode is positional).
    pub mount: i8,
    /// Strikeable hit points (plans/item-strikeable.md): a hit chips this via `PunchableFace::
    /// absorb`, the item dies at `<= 0`. Born at spawn from `items::hurt::item_hp(kind)` --
    /// `0.0` = a kind that opts out of being strikeable (stations, badges, projectiles, `None`).
    /// Appended at the struct's END (bincode is positional; never reorder the fields above).
    pub hp: f32,
}

impl Item {
    pub const EMPTY: Self = Self {
        kind: ItemKind::None,
        pos: Vector2::ZERO,
        vel: Vector2::ZERO,
        owner: -1,
        gas: 0.0,
        gas_max: 0.0,
        timer: 0,
        facing: 1.0,
        tool: ToolKind::TrailPen,
        stroke: 0,
        thrown: false,
        mount: -1,
        hp: 0.0, // unstrikeable by default; spawn sites set it from `items::hurt::item_hp(kind)`
    };
    pub fn active(&self) -> bool {
        !matches!(self.kind, ItemKind::None)
    }
}

// --- items ---------------------------------------------------------------------------------------

const HOLD_OFFSET: Vector2 = Vector2::new(34.0, -56.0); // held item position relative to fighter feet
const BOLT_R: f32 = 12.0; // laser bolt collision radius
pub(crate) const ITEM_R: f32 = 30.0; // pickup reach: ground item within this of the body is grabbable
const DROP_TOSS_X: f32 = 180.0; // forward velocity given to a dropped item
const DROP_TOSS_Y: f32 = -120.0; // small upward pop on drop (negative = up); gravity arcs it down
/// TetrisDropper spawn offset, facing side: one piece-width-ish ahead of the fighter's feet.
const TETRIS_DROP_OFFSET_X: f32 = TETRIS_CELL * 2.5;
/// TetrisDropper spawn height above the fighter's feet (negative-y-up): enough clearance for the
/// tallest piece (the L/S shapes are 3 cells tall) to fall clear before any part crosses the floor.
const TETRIS_DROP_ABOVE_Y: f32 = 200.0;
/// A hair of initial downward velocity for a dropped piece -- `InkPath::traveling()` requires
/// `vel != ZERO`, so a true zero would never enter `integrate_ink` at all and just hang there.
/// This is well under one px/frame; gravity takes over from here exactly like the arc shot.
const TETRIS_DROP_SEED_VY: f32 = 0.5;
/// Vertical half-height of the pickup box: on the order of one platform-drop height (STAGE0's
/// main floor -> side platform gap is 185px) so an item resting on a thin platform slightly
/// above/below the body is still reachable, while the top platform (350px up) is not.
pub(crate) const PICKUP_VERT_TOL: f32 = 200.0;
/// Catching a thrown item in flight is HARDER than picking one off the ground: the catch pass
/// shrinks the throw's contact circle (`throw_item.hit.r`) by this scale, so a graze that would
/// still damage sails past an open hand -- only a near-center arrival is snatchable.
pub(crate) const CATCH_R_SCALE: f32 = 0.6;

/// Every `item_spawn_interval` ticks, drop a weighted-random item into a free slot. Position is
/// chosen from the LCG so it is identical on both peers.
pub(crate) fn maybe_spawn_item(n: &mut SimState, t: &Tune) {
    if !t.items_on || t.item_spawn_interval <= 0 || n.tick == 0 {
        return;
    }
    if n.tick % (t.item_spawn_interval as u64) != 0 {
        return;
    }
    // one item at a time (default on): skip the drop if any pickup already exists (ground OR held).
    // Projectiles don't count, so a bolt in flight never blocks the next gun. Generic over kinds.
    if t.one_item_at_a_time
        && n.items
            .iter()
            .any(|it| it.active() && !it.kind.is_projectile() && it.mount < 0)
    {
        return; // a mounted station (mount >= 0) is a permanent fixture, not the field's one pickup
    }
    let Some(slot) = n.items.iter().position(|it| !it.active()) else {
        return; // field full
    };
    // weighted kind pick across the gun table. Add kinds here as they land.
    let table = [
        (ItemKind::LaserGun, t.laser.spawn_weight),
        (ItemKind::BobGun, t.bomb.spawn_weight),
        (ItemKind::Pen, t.ink_spawn_weight),
        (ItemKind::TetrisGun, t.tetris.spawn_weight),
        (ItemKind::InkGun, t.ink_spawn_weight), // drawn-shot gun spawns like the pen for now
        (ItemKind::WingsBadge, t.badge_spawn_weight),
        (ItemKind::AcCore, t.ac_spawn_weight),
        // shares the arc gun's knob: a tetris-family drop is as likely as the tetris-family lob.
        (ItemKind::TetrisDropper, t.tetris.spawn_weight),
    ];
    let total: f32 = table.iter().map(|&(_, w)| w.max(0.0)).sum();
    if total <= 0.0 {
        return;
    }
    let roll = (roll_rng(n) % 100_000) as f32 / 100_000.0 * total;
    let mut acc = 0.0;
    let mut kind = table[0].0;
    for &(k, w) in &table {
        acc += w.max(0.0);
        if roll < acc {
            kind = k;
            break;
        }
    }
    let gas = fresh_gas(kind, t);

    let span = (FLOOR_RIGHT - FLOOR_LEFT - 120.0).max(0.0);
    let frac = (roll_rng(n) % 1000) as f32 / 1000.0;
    let x = FLOOR_LEFT + 60.0 + frac * span;
    n.items[slot] = Item {
        kind,
        pos: Vector2::new(x, GROUND_Y - 240.0), // drop in from above
        gas,
        gas_max: gas,
        hp: crate::v1::items::hurt::item_hp(kind), // strikeable ground pickup (0 = opts out)
        facing: 1.0,
        // tool/stroke: default preset; roll them once >1 pen material ships. owner -1, vel/timer/
        // thrown/mount take their free-ground-item defaults from EMPTY.
        ..Item::EMPTY
    };
}

/// A fresh item's starting `gas` -- the general first-dimension use measure: gun shots, or a pen's
/// ink-length budget. Spawn sets both `gas` and `gas_max` to this so a HUD can normalize gas/gas_max.
fn fresh_gas(kind: ItemKind, t: &Tune) -> f32 {
    match kind {
        ItemKind::BobGun => t.bomb.ammo as f32,
        ItemKind::TetrisGun | ItemKind::TetrisDropper => t.tetris.ammo as f32,
        ItemKind::Pen | ItemKind::InkGun => t.ink_budget,
        ItemKind::WingsBadge | ItemKind::AcCore => 1.0, // consumed whole on attach
        _ => t.laser.ammo as f32,
    }
}

/// Force-drop one item of a chosen kind into a free slot (the menu's debug spawn). Lands mid-stage,
/// dropping in from above like a natural spawn. `tool`/`stroke` are the pen loadout off the menu
/// card (guns ignore them). No-op when the field is full.
pub fn spawn_kind(n: &mut SimState, kind: ItemKind, tool: ToolKind, stroke: StrokeId, t: &Tune) {
    let Some(slot) = n.items.iter().position(|it| !it.active()) else {
        return;
    };
    let gas = fresh_gas(kind, t);
    let x = (FLOOR_LEFT + FLOOR_RIGHT) * 0.5;
    n.items[slot] = Item {
        kind,
        pos: Vector2::new(x, GROUND_Y - 240.0),
        gas,
        gas_max: gas,
        hp: crate::v1::items::hurt::item_hp(kind), // strikeable ground pickup (0 = opts out)
        facing: 1.0,
        tool,
        stroke,
        ..Item::EMPTY // owner -1, vel/timer/thrown/mount at their free-ground-item defaults
    };
}

/// Clear every item slot and unhand both fighters (the menu's "clear field" button).
/// Mounted station fixtures (`mount >= 0`, the ship's helm) survive: they are terrain
/// furniture seeded at spawn like the hull itself, not field pickups -- wiping one would
/// strand the ship unpilotable for the rest of the match.
pub fn clear_items(n: &mut SimState) {
    for it in &mut n.items {
        if it.mount < 0 {
            *it = Item::EMPTY;
        }
    }
    for f in &mut n.fighters {
        f.holding = -1;
    }
}

/// Nearest unowned ground pickup (gun OR pen) reachable by a grounded, actionable fighter.
/// None in the air / during hitstun so attack stays an aerial -- pickup triggers off ATTACK
/// near an item, and this grounded gate is the ONLY reason an airborne attack still reads
/// as an aerial instead of a grab. The air-grab pickup path (za_warudo's item-intents block)
/// does NOT go through this gate -- it calls `nearest_pickup_reach` directly.
pub fn nearest_pickup(f: &Fighter, items: &[Item; MAX_ITEMS], t: &Tune) -> Option<usize> {
    if airborne(f.state) || f.hitstun != 0 || f.hitlag != 0 {
        return None;
    }
    nearest_pickup_reach(f, items, t)
}

/// The reach geometry only, no grounded/airborne gate: shared by `nearest_pickup` (grounded
/// attack-pickup, above) and the airborne GRAB-pickup intent. The reach is a box centered on
/// the hurtbox/ECB anchor -- symmetric front/back (`|dx| <= t.pickup_reach`; facing plays no
/// part, an item just behind the heels is as claimable as one ahead) and one platform-drop
/// tall each way (`|dy| <= PICKUP_VERT_TOL`), so thin-platform height differences are in
/// reach without any occlusion check (platforms never block a pickup).
pub(crate) fn nearest_pickup_reach(
    f: &Fighter,
    items: &[Item; MAX_ITEMS],
    t: &Tune,
) -> Option<usize> {
    let (bc, _br) = hurtbox(f);
    items.iter().position(|it| {
        it.active()
            && it.owner < 0
            && (it.kind.is_held_tool() || it.kind.badge().is_some())
            && (it.pos.x - bc.x).abs() <= t.pickup_reach
            && (it.pos.y - bc.y).abs() <= PICKUP_VERT_TOL
    })
}

/// Held gun + fire intent: spawn the gun's projectile if off cooldown with ammo, decrement, vanish
/// when spent. Laser fires a bolt (auto-fire = weak); the red gun lobs an arcing bomb.
/// `auto` (held, not a fresh tap) marks a laser bolt weak via its `ammo` slot. `aim` is the
/// unit c-stick direction from the intent scan (ItemLogic.aims), ZERO = fire along facing.
/// `aim_y` is the raw main-stick y at the press -- ignored by every gun except TetrisDropper,
/// which quantizes it into a piece pick (`items::tetris_drop::shape_from_aim_y`).
// parity(v1-ink-tools-and-loadouts): pen, anchored ink gun, tetromino lob, and tetromino drop resolve their tool, material row, gas, aim, shape, spawn, and spent lifecycle from the item loadout
pub(crate) fn fire_gun(
    n: &mut SimState,
    idx: usize,
    auto: bool,
    aim: Vector2,
    aim_y: f32,
    t: &Tune,
) {
    let holding = n.fighters[idx].holding;
    if holding < 0 {
        return;
    }
    let k = holding as usize;
    let gun = n.items[k].kind;
    if !gun.is_gun() || n.items[k].timer > 0 || n.items[k].gas < 1.0 {
        return; // wrong item, on cooldown, or empty — the intent fired but nothing comes out
    }
    let cfg = match gun {
        ItemKind::BobGun => &t.bomb,
        ItemKind::TetrisGun | ItemKind::TetrisDropper => &t.tetris,
        _ => &t.laser,
    };
    let f = n.fighters[idx];
    // shot direction: the aim stick, falling back to facing on neutral. Aim never turns
    // the character — only the shot.
    let dir = if aim.length_squared() > 0.01 {
        aim.normalize_or_zero()
    } else {
        Vector2::new(f.facing, 0.0)
    };
    // facing sign for the projectile's hit-angle mirror (and sprite flip)
    let dir_facing = if dir.x == 0.0 {
        f.facing
    } else if dir.x > 0.0 {
        1.0
    } else {
        -1.0
    };
    let muzzle = f.pos + Vector2::new((HOLD_OFFSET.x + 20.0) * f.facing, HOLD_OFFSET.y);
    if gun == ItemKind::TetrisGun || gun == ItemKind::TetrisDropper {
        // the shot IS ink: the piece claims a path slot (not an item slot) and is Traveling from
        // birth — integrate_ink arcs/drops it, stacks it, locks it into standable/strikeable
        // terrain. TetrisDropper is TetrisGun's sibling (items/tetris_drop.rs): same slot search,
        // same tetromino table, same settle/bake path below -- only the fire-time shape pick and
        // spawn geometry differ, branched right where they're used.
        // A full board evicts by the shared policy (stage/board.rs): oldest settled EXPIRING
        // player stroke first, permanent pieces only as the last resort (never a baked stage
        // stroke, never ink mid-draw or in flight); if even that fails, keep the ammo — the
        // shot never happened.
        let Some(slot) = crate::v1::stage::board::claim_stroke_slot(&n.paths, &n.nodes) else {
            return;
        };
        n.paths[slot].release(&mut n.free); // evicted: reclaim the loser's span before building the piece
        let props = t.strokes.get(n.items[k].stroke);
        // Built into a local (not assigned inline): the mortar branch's `roll_rng(n)` needs `&mut n`
        // whole, which can't coexist with the `&mut n.nodes`/`&mut n.free` `tetromino_path` claims.
        let new_path = if gun == ItemKind::TetrisDropper {
            // pure vertical drop, one piece-width ahead on the FACING side (not the aim stick --
            // aim never turns the shot for this kind, only the piece pick does): a near-ZERO
            // initial velocity, so integrate_ink's gravity is the only thing that ever touches
            // vel.y -- vel.x stays 0 for the whole fall, i.e. a perfectly straight drop.
            let shape = crate::v1::items::tetris_drop::shape_from_aim_y(aim_y);
            let at = f.pos + Vector2::new(TETRIS_DROP_OFFSET_X * f.facing, -TETRIS_DROP_ABOVE_Y);
            let vel = Vector2::new(0.0, TETRIS_DROP_SEED_VY);
            tetromino_path(
                shape,
                at,
                vel,
                props,
                idx as i8,
                n.tick,
                &mut n.nodes,
                &mut n.free,
            )
        } else {
            // steep mortar lob (ink vel is px/frame); aimed fire tilts the lob, neutral matches
            // the classic up-and-forward. Birth spin toward travel = the tumbling ball. Shape
            // rolls off the sim's LCG so both peers lob the same piece. Spawn a bit above the
            // muzzle so a tall piece's bottom edge starts clear of the floor it must cross.
            let shape = (roll_rng(n) % TETROMINO_SHAPES as u64) as u8;
            let vel = (dir * 0.8 + Vector2::new(0.0, -1.0)) * cfg.speed * DT;
            let at = muzzle + Vector2::new(0.0, -40.0);
            let mut p = tetromino_path(
                shape,
                at,
                vel,
                props,
                idx as i8,
                n.tick,
                &mut n.nodes,
                &mut n.free,
            );
            p.omega = dir_facing * 0.12;
            p
        };
        n.paths[slot] = new_path;
    } else if let Some(slot) = n.items.iter().position(|x| !x.active()) {
        let (kind, vel) = if gun == ItemKind::BobGun {
            // lob along the aim with the same up bias; gravity bends it into an arc.
            // Neutral (dir = facing) reproduces the classic up-and-forward exactly.
            (
                ItemKind::Bomb,
                dir * cfg.speed + Vector2::new(0.0, -cfg.speed * 0.5),
            )
        } else {
            (ItemKind::LaserBolt, dir * cfg.speed) // bolts fly angled now
        };
        n.items[slot] = Item {
            kind,
            pos: muzzle,
            vel,
            owner: idx as i8,
            gas: auto as i64 as f32, // laser: 1 = auto-fire (weak), 0 = full power; bomb ignores this
            gas_max: 1.0,
            hp: crate::v1::items::hurt::item_hp(kind), // projectile kinds opt out (0.0) -- explicit anyway
            timer: cfg.range,
            facing: f.facing,
            ..Item::EMPTY // tool/stroke/thrown/mount default (a fired bolt is never mounted/thrown)
        };
    }
    n.items[k].gas -= 1.0;
    n.items[k].timer = if auto { cfg.autofire_cd } else { cfg.cooldown };
    if n.items[k].gas < 1.0 {
        n.items[k] = Item::EMPTY; // spent gun vanishes
        n.fighters[idx].holding = -1;
    }
}

/// Drop intent: detach the held item to the ground with a small forward toss (update_items arcs it).
pub(crate) fn drop_item(n: &mut SimState, idx: usize) {
    let holding = n.fighters[idx].holding;
    if holding < 0 {
        return;
    }
    let k = holding as usize;
    let f = n.fighters[idx];
    n.items[k].owner = -1;
    n.items[k].vel = Vector2::new(f.facing * DROP_TOSS_X, DROP_TOSS_Y) + f.vel;
    n.fighters[idx].holding = -1;
}

/// Throw intent (directional grab): launch the held item as a live projectile in the stick
/// direction. `owner` stays the thrower so the armed item skips its own body; `thrown` arms it.
/// Speeds + the contact hitbox are `Tune.throw_item` (panel-editable, generous by default).
pub(crate) fn throw_item(n: &mut SimState, idx: usize, dir: ThrowDir, t: &Tune) {
    let holding = n.fighters[idx].holding;
    if holding < 0 {
        return;
    }
    let k = holding as usize;
    let f = n.fighters[idx];
    let ti = t.throw_item;
    let vel = match dir {
        ThrowDir::Up => Vector2::new(0.0, -ti.up_speed),
        ThrowDir::Down => Vector2::new(0.0, ti.down_speed),
        ThrowDir::Forward => Vector2::new(f.facing * ti.fwd_speed, 0.0),
        ThrowDir::Back => Vector2::new(-f.facing * ti.back_speed, 0.0),
    } + f.vel; // momentum transfer: a dash-throw carries the run (glide toss)
    n.items[k].vel = vel;
    // the armed hit launches along the item's travel, so facing follows the throw,
    // not the thrower (a back throw hits backward).
    n.items[k].facing = if vel.x.abs() > 1.0 {
        vel.x.signum()
    } else {
        f.facing
    };
    n.items[k].thrown = true; // owner kept = thrower: the armed item passes through its own thrower
    n.fighters[idx].holding = -1;
}

/// An armed thrown item connecting with a non-thrower: damage + knockback from `throw_item.hit`,
/// launched along the item's actual travel (Carry) so an up throw pops the victim UP and a down
/// throw spikes — the box's `angle` only covers the near-stationary fallback. Mirrors the bolt's
/// own hit in `items::laser_gun` (`straight_burn_tick`). No interrupt: item hits historically
/// don't cancel the victim's mid-swing move.
/// Pure builder: the throw-hit strike as an `ItemAct::Strike` (write carried out by
/// `acts::apply_item_act`). `from` is the victim's hurtbox center (what the old inline strike read).
fn throw_hit_act(fighter: i8, vel: Vector2, facing: f32, from: Vector2, t: &Tune) -> ItemAct {
    let atk = t.throw_item.hit;
    let aim = if vel.length() > 1.0 {
        Aim::Carry { vel, up: 0.15 }
    } else {
        Aim::Angle {
            deg: atk.angle,
            facing,
        }
    };
    ItemAct::Strike {
        fighter,
        hb: atk,
        dmg: atk.damage,
        kb_scale: 1.0,
        hitlag_bonus: 2,
        interrupt: false,
        aim,
        from,
        follow: StrikeFollow::None,
    }
}

/// Pickup intent: claim the nearest reachable unowned ground item. A badge attaches
/// instead of being held: the item is consumed on the spot and the fighter's badge bit
/// turns on — no hand slot, so it can never be dropped, thrown, or knocked loose.
/// Uses the ungated reach test: `Act::Pickup` only ever comes from a caller (za_warudo's
/// item-intents block) that already checked the right gate for its path -- grounded for an
/// ATTACK-triggered pickup, airborne-allowed for a GRAB-triggered one -- so re-applying the
/// grounded-only `nearest_pickup` gate here would silently drop the airborne case.
pub(crate) fn pickup_item(n: &mut SimState, idx: usize, t: &Tune) {
    let f = n.fighters[idx];
    if let Some(k) = nearest_pickup_reach(&f, &n.items, t) {
        if let Some(b) = n.items[k].kind.badge() {
            attach_badge(n, idx, b);
            n.items[k] = Item::EMPTY;
            return;
        }
        n.items[k].owner = idx as i8;
        n.fighters[idx].holding = k as i8;
        // the press that claimed it must be RELEASED before it can fire/draw (no
        // auto-trigger on pickup); cleared in the scan once attack is fully up.
        n.fighters[idx].pickup_hold = true;
    }
}

/// What a free item's floor contact does. Armed throws disarm on landing regardless of
/// kind — that's throw-state policy, not kind policy.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Land {
    Settle,   // rest on the surface, become a ground pickup
    Detonate, // bombs
    Ignore,   // bolts: terrain never stops them (ricochet is a later body-bus step)
}

/// Per-kind physics/contact rows: Melee's ItemLogicTable, flattened to data
/// (plans/body-bus.md). `punchable` / `aims` / `draws` join in later steps; a new item
/// should be an ItemCard row + a row here + a sprite, not a new match arm.
#[derive(Copy, Clone)]
pub struct ItemLogic {
    pub gravity: bool,
    pub land: Land,
    /// C-stick aims this held weapon (arrow tell + directional fire); suppresses c-attacks.
    pub aims: bool,
    /// The attack button draws ink instead of firing (pen-style hold semantics).
    pub draws: bool,
    /// Ink slam reflects it across the incident normal instead of shoving it (bolts).
    pub ricochet: bool,
    /// Ink slam detonates it on the spot (bombs).
    pub boom_on_hit: bool,
}

/// Thin wrapper over `behavior::spec_for` (mod-api.md Tier 0): each kind's own file sets
/// these fields once in its `spec()`; this just reshapes them into the legacy row every
/// caller (za_warudo, ac, stage, the shell) already reads.
pub fn item_logic(k: ItemKind) -> ItemLogic {
    let s = behavior::spec_for(k);
    ItemLogic {
        gravity: s.gravity,
        land: s.land,
        aims: s.aims,
        draws: s.draws,
        ricochet: s.ricochet,
        boom_on_hit: s.boom_on_hit,
    }
}

/// The one crossed-from-above check for every item arc (thrown, unowned, bomb): sweep
/// the shared soup while descending. `Land::Ignore` rows never land.
pub(crate) fn item_landing(it: &Item, pos: Vector2, vel: Vector2, soup: &Soup) -> Option<FloorHit> {
    if item_logic(it.kind).land == Land::Ignore || vel.y < 0.0 {
        return None;
    }
    sweep_floors(it.pos, pos, false, soup.surfs())
}

/// Post-step item physics: ground guns fall + rest, held guns follow their owner (dropping if the
/// owner died/respawned), bolts fly + hit + expire.
pub(crate) fn update_items(n: &mut SimState, t: &Tune) -> ItemActs {
    let np = (n.active as usize).min(MAX_PLAYERS);
    // items sweep the same soup as fighters (body-bus); `None` -- the hull-containment ink
    // scope (plans/ship-containment.md §4) is a per-body position filter and items are swept
    // here as a shared batch, not one at a time, so this pass is unscoped, same as before.
    let soup = Soup::collect(&n.paths, &n.nodes, None);

    // Per-badge meter maintenance (ac_gas drain + wear::tick) moved to fighters::tick_badge_meters
    // (Phase C1): it reads/writes only the fighter, so items no longer own it. Called from the Items
    // phase just before this fn, preserving the exact pipeline point.

    // Fighter effects (catch, badge attach) are emitted as ItemAct descriptors and applied AFTER
    // this pass by acts::ApplyItemActs, so this fn never writes a fighter. `pending_claimed` shadows
    // the deferred `holding`/`catch_win` catch write so a second thrown item this same pass cannot
    // double-claim a fighter the way the old immediate write prevented (verified read-back: the
    // catch gate at 789/788 is the only within-pass reader of those fields).
    let mut acts = ItemActs::EMPTY;
    let mut pending_claimed = [false; MAX_PLAYERS];
    for k in 0..MAX_ITEMS {
        let it = n.items[k];
        if !it.active() {
            continue;
        }
        if it.mount >= 0 {
            continue; // mounted station (crate::v1::station): inert fixture — skip physics/decay/hit
            // (station::repin_mounted, after ink integration, is what moves it)
        }
        if it.kind.is_held_tool() && it.thrown {
            // armed throw in flight: arc under gravity, damage the first non-thrower it grazes,
            // then despawn. Landing without a hit disarms it into a normal unowned ground pickup.
            let mut v = it.vel;
            v.y += t.gravity * DT;
            let p = it.pos + v * DT;
            n.items[k].pos = p;
            n.items[k].vel = v;
            let landing = item_landing(&it, p, v, &soup);

            // catch precedence: a free-handed fighter whose grab is live catches the item INSTEAD
            // of taking the throw hit -- checked before the damage sweep below, for every fighter
            // the item's contact circle touches (including the thrower: their own boomeranging
            // throw is catchable too, same as anyone else with a free hand). "Grab is live" is
            // EITHER CharState::Grab's active window (grounded, the same catchable window
            // `resolve_grab` uses for a fighter-grab catch) OR the airborne GRAB press's
            // `catch_win` latch (za_warudo's CATCH_WINDOW). Respects the same hitstun/hitlag/
            // intangible ghost gates AcCore's touch-attach does, plus the one-item-at-a-time
            // hand check. The contact circle is TIGHTER than the throw hit's (CATCH_R_SCALE):
            // a graze that damages is not close enough to snatch. Per-kind opt-out via
            // `ItemSpec.catchable` (the TetrisGun ships uncatchable).
            let mut claimed: Option<usize> = None;
            if behavior::spec_for(it.kind).catchable {
                for fi in 0..np {
                    let (bc, br) = hurtbox(&n.fighters[fi]);
                    if !geo::circles_touch(p, t.throw_item.hit.r * CATCH_R_SCALE, bc, br) {
                        continue;
                    }
                    let f = &n.fighters[fi];
                    let active = (f.state == CharState::Grab
                        && f.frame >= t.grab_startup
                        && f.frame < t.grab_startup + t.grab_active)
                        || f.catch_win > 0;
                    if f.holding < 0
                        && !pending_claimed[fi] // an earlier item this pass already claimed this fighter
                        && f.invuln == 0
                        && !f.intangible
                        && f.hitstun == 0
                        && f.hitlag == 0
                        && active
                    {
                        claimed = Some(fi);
                        break; // first in slot order (deterministic); only one fighter can catch it
                    }
                }
            }
            if let Some(fi) = claimed {
                n.items[k].thrown = false;
                n.items[k].owner = fi as i8;
                n.items[k].vel = Vector2::ZERO;
                // the holding/pickup_hold/catch_win writes are deferred to ApplyItemActs; shadow the
                // claim so a later thrown item this pass can't catch onto the same fighter.
                acts.push(ItemAct::Caught {
                    fighter: fi as i8,
                    item: k as i8,
                });
                pending_claimed[fi] = true;
                continue; // claimed: no damage, no ink billiard, no landing resolution this frame
            }

            let mut hit_someone = false;
            for fi in 0..np {
                if fi as i8 == it.owner {
                    continue; // your own throw passes through you
                }
                let (bc, br) = hurtbox(&n.fighters[fi]);
                if geo::circles_touch(p, t.throw_item.hit.r, bc, br) {
                    // strike write relocated to acts::apply_item_act; `bc` is the same contact point
                    // apply_item_throw_hit read via `hurtbox(b).0`. Applied immediately: bit-identical.
                    apply_item_act(n, throw_hit_act(fi as i8, v, it.facing, bc, t), t);
                    hit_someone = true;
                }
            }
            // an armed throw whacks ink bodies too (billiards): same contact box, spent on impact
            let ti = t.throw_item.hit;
            if strike_ink(
                &mut n.paths,
                p,
                ti.r,
                &ti,
                ti.damage,
                it.facing,
                &n.nodes,
                t,
            ) {
                hit_someone = true;
            }
            if out_of_bounds(p) && !item_zone_exempt(it.kind, t) {
                // crossed a blast zone: quiet despawn. Items always read the STATIC frame here
                // (out_of_bounds), never `Tune::zone_mode` -- the live/toggleable zone is fighter-only.
                n.items[k] = Item::EMPTY;
            } else if hit_someone {
                n.items[k] = Item::EMPTY; // spent on impact
            } else if let Some(h) = landing {
                // landed harmlessly (platform, ink, or the main floor — one sweep): the GENERIC
                // SOLVE always runs (plans/body-unify.md step 3, items/floor.rs), then its settle
                // override decides whether the contact disarms back into a normal unowned ground
                // item. Every kind reaching this arm has a restitution-0 row except the pen, so
                // `settled` is true on the very first contact for them -- same as before. A pen
                // that hasn't finished rebounding stays `thrown` and re-enters this same arm next
                // frame, bouncing until its rebound decays.
                let (pos, vel, settled) = floor::resolve_floor_contact(it.kind, p, v, &h);
                n.items[k].pos = pos;
                n.items[k].vel = vel;
                if settled {
                    n.items[k].thrown = false;
                    n.items[k].owner = -1;
                }
            }
        } else if it.kind.is_held_tool() && it.owner >= 0 {
            let o = it.owner as usize;
            if n.fighters[o].holding != k as i8 {
                n.items[k].owner = -1; // owner let go / died: drop to the ground where it is
            } else {
                let f = n.fighters[o];
                // parity(v1-held-item-relative-follow): held tools follow their owner's faced offset every tick
                n.items[k].pos = f.pos + Vector2::new(HOLD_OFFSET.x * f.facing, HOLD_OFFSET.y);
                n.items[k].facing = f.facing;
                if n.items[k].timer > 0 {
                    n.items[k].timer -= 1; // tick the fire cooldown while held
                }
            }
        } else if it.kind.is_held_tool() || it.kind.badge().is_some() {
            // unowned: gravity, settle on whatever floor catches it — platforms and drawn
            // ink included now, not just the main span. Off every edge there is no floor,
            // so it keeps falling and despawns at the blast zone. (A badge only ever runs
            // this arm: pickup consumes it, so it can't be held or thrown.)
            let mut p = it.pos;
            let mut v = it.vel;
            p += v * DT;
            let mut rested = false;
            if let Some(h) = item_landing(&it, p, v, &soup) {
                // GENERIC SOLVE + settle override (plans/body-unify.md step 3, items/floor.rs):
                // a restitution-0 row (every kind except the pen) settles dead-stop on this very
                // first contact, same as the old unconditional branch it replaces; the pen's row
                // can instead come back not-`rested`, in which case it keeps falling/bouncing and
                // re-enters this same arm next frame against whatever floor it meets next.
                let (pos, vel, settled) = floor::resolve_floor_contact(it.kind, p, v, &h);
                p = pos;
                v = vel;
                rested = settled;
            } else {
                v.y += t.gravity * DT; // no floor under us: keep falling
            }
            n.items[k].pos = p;
            n.items[k].vel = v;
            if out_of_bounds(p) && !item_zone_exempt(it.kind, t) {
                // crossed a blast zone: quiet despawn. Static frame only -- see the comment on the
                // thrown-item despawn above; the live/toggleable zone never applies to items.
                n.items[k] = Item::EMPTY;
            } else if rested && it.kind.despawn_when_spent() && it.gas < 1.0 {
                // an empty pen's "unload": once it's idle on the ground with no ink left it
                // despawns (unlike a spent gun, which vanishes the instant it empties). It stays
                // pickup-able while it falls/settles; guns keep resting here forever.
                n.items[k] = Item::EMPTY;
            }
        } else {
            // every remaining kind's per-tick logic lives in its own file under core/src/items/,
            // behind the sealed ItemBehavior trait (plans/mod-api.md Tier 0); this is the one
            // dispatch + one applier (`ItemFx`) that replaces what used to be five separate
            // match arms here (LaserBolt/Bomb/Rocket/PlasmaBall/AcCore).
            let mut cx = behavior::ItemCx {
                n: &mut *n,
                t,
                soup: &soup,
                np,
            };
            let (next, fx) = behavior::dispatch_tick(it.kind, it, &mut cx);
            match fx {
                behavior::ItemFx::None => n.items[k] = next,
                behavior::ItemFx::Despawn => n.items[k] = Item::EMPTY,
                behavior::ItemFx::Explode(p) => {
                    explode(n, p, t);
                    n.items[k] = Item::EMPTY;
                }
                behavior::ItemFx::Attach {
                    fighter,
                    badge,
                    arm,
                    fx_at,
                } => {
                    // fighter writes (badge/arm/arm_cd/qb_cd) deferred to ApplyItemActs; no within-pass
                    // read-back on those fields, so the item write + fx stay inline here.
                    acts.push(ItemAct::AttachBadge {
                        fighter: fighter as i8,
                        badge,
                        arm,
                    });
                    n.items[k] = Item::EMPTY;
                    push_fx(n, FxKind::Transform, fx_at);
                }
            }
        }
    }
    acts
}

/// Item's effective body radius when a flying ink line slams it (contact + impulse).
const ITEM_BODY_R: f32 = 16.0;
/// Point-mass an item presents to the billiard solver. Light next to a drawn line's
/// hundreds of mass units: big pieces send items flying.
const ITEM_MASS: f32 = 150.0;

/// Traveling ink slams items (plans/body-bus.md step 6): per the kind's row, a bomb
/// detonates on the spot, a bolt ricochets across the incident normal ("shoot the
/// flying piece and eat your own laser"), everything else takes a rigid impulse and
/// flies. Held items are pinned to hands and skip the pass; the same truck-speed gate
/// as fighters keeps a slow drift from popping bombs.
// parity(v1-ink-item-hazard): traveling ink detonates, ricochets, or shoves items by authored item behavior while held items and slow strokes are excluded
pub(crate) fn ink_hits_items(n: &mut SimState, t: &Tune) {
    for pi in 0..crate::v1::stage::MAX_DRAWN {
        let p = n.paths[pi];
        if !p.traveling() || p.vel.length() < crate::v1::stage::INK_TRUCK_SPEED {
            continue;
        }
        for k in 0..MAX_ITEMS {
            let it = n.items[k];
            if !it.active() {
                continue;
            }
            let held = it.kind.is_held_tool() && it.owner >= 0 && !it.thrown;
            if held {
                continue;
            }
            let logic = item_logic(it.kind);
            let r = if it.kind == ItemKind::LaserBolt {
                BOLT_R
            } else {
                ITEM_BODY_R
            };
            // closest ink segment point to the item's body
            let mut best: Option<(Vector2, f32)> = None;
            for s in 0..(p.len as usize).saturating_sub(1) {
                let (a, b) = p.world_seg(s, &n.nodes);
                let q = geo::closest_on_seg(it.pos, a, b);
                let d = (it.pos - q).length();
                if best.map_or(true, |(_, bd)| d < bd) {
                    best = Some((q, d));
                }
            }
            let Some((q, d)) = best else { continue };
            if d > r + crate::v1::stage::INK_BODY_R {
                continue;
            }
            let normal = if d > 1e-3 {
                (it.pos - q) / d
            } else {
                Vector2::new(0.0, -1.0)
            };
            if logic.boom_on_hit {
                explode(n, it.pos, t);
                n.items[k] = Item::EMPTY;
            } else if logic.ricochet {
                // full-restitution reflection: the bolt keeps its speed, flips course
                n.items[k].vel = geo::reflect(it.vel, normal, 1.0);
                n.items[k].facing = if n.items[k].vel.x >= 0.0 { 1.0 } else { -1.0 };
            } else {
                // rigid shove, item as a point mass (px/frame inside the solver)
                let mut ink_bits = p.body_bits(&n.nodes);
                let mut item_bits = crate::v1::body::BodyBits {
                    pos: it.pos,
                    vel: it.vel * DT,
                    omega: 0.0,
                    inv_mass: 1.0 / ITEM_MASS,
                    inv_inertia: 0.0,
                };
                // normal from ink toward the item = a→b with the ink as a
                crate::v1::body::collide(&mut ink_bits, &mut item_bits, q, normal, 0.5, 0.2);
                crate::v1::stage::write_billiard(&mut n.paths[pi], &ink_bits);
                n.items[k].vel = item_bits.vel / DT;
            }
        }
    }
}

/// Detonate the bomb: every fighter inside `blast_r` takes damage + radial knockback (away from the
/// center, biased upward so it pops), scaled by `knockback_mult` and distance falloff. Spawn i-frames
/// and active dodges shrug it off. Hits the thrower too -- standing in your own blast is on you.
fn explode(n: &mut SimState, center: Vector2, t: &Tune) {
    push_fx(n, FxKind::Explosion, center); // the shell paints the funni fireball here
    let atk = t.bomb.hit;
    let np = (n.active as usize).min(MAX_PLAYERS);
    for fi in 0..np {
        let (bc, _) = hurtbox(&n.fighters[fi]);
        let dist = (bc - center).length();
        if dist > t.bomb.blast_r {
            continue;
        }
        let falloff = blast_falloff(dist, t.bomb.blast_r); // full at center, ~half at the rim
        // falloff scales the formula OUTPUT too (kb_scale), not just the damage.
        // No interrupt: blast hits historically don't cancel the victim's mid-swing move.
        // strike write + the on-hit arm_hits tail relocate to acts::apply_item_act; falloff/aim stay
        // here. Applied immediately per struck fighter (one act each): bit-identical.
        apply_item_act(
            n,
            ItemAct::Strike {
                fighter: fi as i8,
                hb: atk,
                dmg: atk.damage * falloff,
                kb_scale: falloff,
                hitlag_bonus: 4,
                interrupt: false,
                aim: Aim::Radial { center, up: 0.4 }, // up-biased pop
                from: bc,
                follow: StrikeFollow::ArmHits,
            },
            t,
        );
    }
    // the blast shoves ink bodies too: radial-ish (the box's up-and-out angle, signed away from the
    // center), damage falls off by distance to the body's rim like the fighter hit does.
    for ink in n.paths.iter_mut() {
        if !ink.active() || ink.mass <= 0.0 || ink.drawing {
            continue;
        }
        let (c, br) = ink.bound_circle(&n.nodes);
        let d = c - center;
        let dist = (d.length() - br).max(0.0);
        if dist > t.bomb.blast_r {
            continue;
        }
        let falloff = blast_falloff(dist, t.bomb.blast_r);
        let facing = if d.x >= 0.0 { 1.0 } else { -1.0 };
        // contact = the rim point facing the blast, so an off-center blast also torques the body
        let contact = c - d.normalize_or_zero() * br;
        resolve_hit_ink(&atk, atk.damage * falloff, facing, contact, ink, t);
    }
}
