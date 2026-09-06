//! Character + feel configuration.
//!
//! Three layers, split for tier-1 modding (plans/mod-api.md §"Tier 1: character-as-file"):
//!   * `CharData`  -- per-character physics in SOURCE units (units/frame @ 60fps).
//!   * `CharSpec`  -- the MOD UNIT: `CharData` + the move tables + the `[SpecialMove; 4]` loadout
//!                    + the per-character kit (grab / shield / charge / walljump / footstool / AC).
//!                    Built-in characters are const presets of this type; `KNEEMAN` is roster row 0.
//!   * `MatchTune` -- match-global, panel-owned knobs (items, spawn rates, kill rules, global feel).
//!
//! `Tune` is the runtime handle the sim threads. It is the flat PIXEL-space view a single fighter
//! resolves to (every field the FSM / physics / combat reads), plus the `roster` of `CharSpec`s and
//! the match config it was built from. `step` calls `Tune::for_char(char_id)` to derive the
//! per-fighter view: physics + moveset come from that char's `CharSpec` row, the match knobs are
//! shared. With every fighter on `KNEEMAN` this reproduces the old single-`Tune` behavior bit for
//! bit. `Tune` stays the ggrs `Config` + handshake payload + egui-panel edit target (net untouched).

use crate::v1::physics::{acc, vel};
use crate::v1::{AttackData, ItemConfig, SpecialMove, StrokeRegistry, ThrowData, ThrowItem};
use serde::{Deserialize, Serialize};

/// Per-character attributes in SOURCE UNITS (units/frame @ 60fps; frames are integers).
/// This is the canonical character definition; the pixel-space fields on `Tune` are derived from it.
///   csv = value taken from the reference physics table
///   est = community-derived value (not in any text dump) — tune freely
///   ult = modern-platform-fighter idea applied on purpose
#[derive(Copy, Clone, Serialize, Deserialize)]
pub struct CharData {
    pub gravity: f32,         // csv 0.13
    pub max_fall: f32,        // csv 2.9   (TerminalVelocity)
    pub fastfall: f32,        // est 3.5   (csv lists 2.9, same as fall — looks like a dup)
    pub walk_max: f32,        // csv 0.85
    pub dash_init: f32,       // est 1.9   (initial dash burst)
    pub run_max: f32,         // est 2.34  (run top speed, dash accelerates toward this)
    pub ground_accel: f32,    // est 0.10
    pub ground_friction: f32, // csv 0.08  (Friction)
    pub fullhop_v: f32,       // est 3.68
    pub shorthop_v: f32,      // est 1.80
    pub airjump_v: f32,       // csv 2.66  (InitDJSpeed)
    pub airjump_h: f32,       // est 1.40  (double-jump horizontal redirect; lets you reverse)
    pub jump_h_init: f32,     // est 0.90  (stick contribution at takeoff)
    pub jump_h_max: f32,      // est 2.50  (takeoff h cap; ABOVE run so momentum survives the jump)
    pub air_speed: f32, // est 1.60  (drift cap; raised from csv 1.12 for control, not momentum cap)
    pub air_accel: f32, // est 0.18  (air mobility; raised from csv 0.06 — classic air is crusty)
    pub air_friction: f32, // csv 0.01  (aerial drag; bleeds excess momentum slowly)
    pub momentum_carry: f32, // est 1.0   (ground->air horizontal momentum mult; 1.0 = full carry)
    pub max_air_jumps: u8, // csv 1     (Jumps)
    pub max_air_dodges: u8, // est 1
    pub roll_speed: f32, // est 1.8
    pub airdodge_speed: f32, // est 3.1   (universal air-dodge burst)
    pub airdodge_drag: f32, // est 0.15  (burst decay so it lunges + settles)
    pub ledgejump_v: f32, // est 2.70
    // frame data (integer frames, not scaled)
    pub jumpsquat: i64,         // ult 3 (universal)
    pub landing_lag: i64,       // est 4
    pub dash_window: i64,       // est 12 (dash-dance window; dash -> run after this)
    pub pivot_frames: i64,      // ult 1
    pub dash_turn_accel: f32, // accel that fights your own momentum when reversing a dash/run/skid
    pub dashstop_friction: f32, // braking friction when a dash/run slides to a stop (Skid)
    pub spotdodge_frames: i64, // est 22
    pub roll_frames: i64,     // est 22
    pub airdodge_frames: i64, // est 28 (then actionable again — Ultimate-style, not helpless)
    pub ledge_intang: i64,    // est 30 (i-frames on grab)
    pub climb_frames: i64,    // est 24 (getup duration)
    pub buffer_frames: i64,   // ult 12 (Ultimate input buffer window)
}

impl CharData {
    pub const KNEEMAN: Self = Self {
        gravity: 0.17, // raised from csv 0.13 — snappier Ultimate-style arc, less hang
        max_fall: 2.9,
        fastfall: 4.2, // raised from csv 2.9 — crisper fast fall, less watery
        walk_max: 0.85,
        dash_init: 1.9,
        run_max: 2.34,
        ground_accel: 0.12,
        ground_friction: 0.22, // raised hard for stopping power (was 0.08 = ice)
        fullhop_v: 3.68,
        shorthop_v: 1.80,
        airjump_v: 3.30, // taller DJ; raised gravity had shrunk every jump's apex
        airjump_h: 1.40,
        jump_h_init: 0.90,
        jump_h_max: 2.50,
        air_speed: 1.60,
        air_accel: 0.22,
        air_friction: 0.01,
        momentum_carry: 1.0,
        max_air_jumps: 1,
        max_air_dodges: 1,
        roll_speed: 1.8,
        airdodge_speed: 3.1,
        airdodge_drag: 0.15,
        ledgejump_v: 2.70,
        jumpsquat: 3,
        landing_lag: 4,
        dash_window: 12,
        pivot_frames: 1,
        dash_turn_accel: 0.50, // reversal brake: bleeds old momentum through 0, no instant flip
        dashstop_friction: 0.30, // grippier than ground_friction (0.22) so a dash brakes hard
        spotdodge_frames: 22,
        roll_frames: 22,
        airdodge_frames: 28,
        ledge_intang: 30,
        climb_frames: 24,
        buffer_frames: 12,
    };
}

/// The mod unit (plans/mod-api.md tier 1): everything that makes one character. Physics (`CharData`)
/// + the normal-attack move tables + the special loadout + the throws + the per-character kit
/// (aerial-read thresholds, grab, weight, smash charge, shield, walljump/footstool/crawl, the AC
/// frame). Built-ins are const presets of this type. The vel/acc-scaled kit fields are in SOURCE
/// units (like `CharData`) so a preset can stay `const`; `Tune::resolve` converts them to pixels.
#[derive(Copy, Clone, Serialize, Deserialize)]
pub struct CharSpec {
    pub phys: CharData,
    // normal-attack move tables (authored directly in pixel/frame space)
    pub jab: AttackData,
    pub nair: AttackData,
    pub fair: AttackData,
    pub bair: AttackData,
    pub uair: AttackData,
    pub dair: AttackData,
    pub dtilt: AttackData,
    pub ftilt: AttackData,
    pub utilt: AttackData,
    pub fsmash: AttackData,
    pub usmash: AttackData,
    pub dsmash: AttackData,
    pub dash_attack: AttackData,
    pub ledge_attack: AttackData,
    pub getup_attack: AttackData,
    pub specials: [SpecialMove; 4], // B-move loadout, indexed by special_slot (N/Side/Up/Down)
    pub throws: [ThrowData; 4],     // fwd / back / up / down
    // aerial read thresholds
    pub dair_threshold: f32, // |aim_y| past this (and steeper than horizontal) picks dair/uair
    pub fastfall_threshold: f32, // stick aim_y must reach this (and beat |dir|) to fast fall
    pub autohop_dmg: f32,    // damage multiplier for auto-short-hop aerials (jump+attack macro)
    // grab -> pummel -> throw
    pub grab_startup: i64,  // wind-up before the grab reach turns on
    pub grab_active: i64,   // frames the grab can catch
    pub grab_recovery: i64, // whiff cool-down (heavy: a missed grab is punishable)
    pub grab_range: f32,    // forward reach of the grab from the body
    pub grab_hold: i64,     // auto-release countdown once a hold lands
    pub grab_mash: i64,     // extra countdown removed per fresh victim input (mash to escape)
    pub pummel_damage: f32, // damage per pummel tap while holding
    pub pummel_bonus: i64,  // hold extended per pummel (capped at grab_hold)
    pub weight: f32,        // victim weight in the KB formula (Falcon/KneeMan-ish ~104)
    pub zone_exempt: bool, // never KO'd by the blast zone (Static or InkExtends); Off already spares everyone
    // smash charge
    pub charge_max: i64, // max frames a smash can bank at the pin
    pub charge_dmg: f32, // damage/kb multiplier at a full bank (linear from 1.0)
    // shield
    pub shield_max: f32, // full shield health: total % it can eat before breaking
    pub shield_regen: f32, // hp per frame recovered while the guard is down
    pub shield_decay: f32, // hp per frame drained while the guard is held up
    pub shieldstun_per_dmg: f32, // frames locked in shield per % blocked (+2 base)
    pub shield_push: f32, // px/s pushback per % blocked (slides the blocker out)
    pub shieldbreak_frames: i64, // dizzy length after a break (fully punishable)
    // walljump / footstool / crawl (SOURCE units; resolve applies vel/acc)
    pub walljump_v: f32, // src: vertical kick off a wall (resolved negative = up)
    pub walljump_h: f32, // src: horizontal kick away from the wall
    pub footstool_v: f32, // src: the jumper's pop off a head (resolved negative = up)
    pub footstool_spike: f32, // src: downward shove on an airborne victim
    pub footstool_stun: i64, // victim stagger frames (grounded victims take half)
    pub crawl_speed: f32, // src: crouched creep speed (well under walk)
    // Armored Core overlay frame (ac.rs owns weapon triggers; these are how the mech moves)
    pub ac_grav_mult: f32, // extreme gravity: multiplier on airborne gravity while AC'd
    pub ac_boost_accel: f32, // src: held-jump thrust accel toward the stick
    pub ac_boost_max: f32, // src: speed cap the boost thrusts toward
    pub ac_qb_speed: f32,  // src: quick-boost burst speed (jump tap)
    pub ac_qb_cd: i64,     // quick-boost cooldown (frames)
}

impl CharSpec {
    /// Roster row 0: Knee Man, the reference character every fighter runs today. Data
    /// lives in `chars/kneeman.rs` (plans/swordsman-lucas.md row 1: "add a character"
    /// means "add a file"); this const forwards so every existing `CharSpec::KNEEMAN`
    /// call site stays byte-identical.
    pub const KNEEMAN: Self = crate::v1::chars::kneeman::spec();

    /// Wrap a bare `CharData` in the default kit (KNEEMAN's moveset). Kept for the tests + the panel
    /// reset path that build a `Tune` straight from a `CharData`.
    pub fn from_data(c: &CharData) -> Self {
        Self {
            phys: *c,
            ..Self::KNEEMAN
        }
    }

    /// The Falcon row (queue-2026-07-03 item 4). Data lives in `chars/falcon.rs`
    /// (plans/swordsman-lucas.md row 1); this fn forwards so every existing
    /// `CharSpec::falcon()` call site stays byte-identical.
    pub fn falcon() -> Self {
        crate::v1::chars::falcon::spec()
    }
}

/// How the frame's fighter blast zone is computed (the live/toggleable blast zone, plans queue).
/// Menu-edited match config, so it lives on `Tune`/`MatchTune`, not `SimState` -- `Tune` sits
/// outside the rollback checksum. Items are UNAFFECTED by this: they always despawn off the static
/// `BLAST_*` frame (`out_of_bounds`), never this mode (see the read site in `step`).
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub enum ZoneMode {
    #[default]
    Static, // the fixed BLAST_* rect (today's behavior)
    InkExtends, // BLAST_* unioned with the live zone-maker ink bounding box (`ink_blast_zone`), if any is down
    Off,        // fighters never KO at the edges this frame (items still do -- see the doc above)
}

/// Match-global, panel-owned config: items, spawn rules, kill formula, and universal "feel" that is
/// NOT a property of a character. Split off `CharSpec` so a character mod cannot change match rules
/// and the egui panel can live-edit the rules without touching a character. Pixel-space; the few
/// vel-scaled fields are resolved in `defaults` so the type is a plain data bag.
#[derive(Copy, Clone, Serialize, Deserialize)]
pub struct MatchTune {
    // input / read feel
    pub smash_window: i64, // attack within this many frames of a hard stick flick = smash, not tilt
    pub b_rev_window: i64, // frames into a special where a back-flick still B-reverses it
    pub di_max_angle: f32, // max degrees the victim's stick can rotate a launch trajectory
    pub coyote_frames: i64, // grace after walking off an edge to still get a full grounded jump
    pub plat_drop_window: i64, // soft-platform drop tilt-window (1 = instant drop / frame-perfect tilt)
    pub zone_mode: ZoneMode, // how the fighter blast zone is computed each frame (menu-toggleable)
    // items (match settings, not character-derived)
    pub items_on: bool,           // master switch for item spawns
    pub item_spawn_interval: i64, // frames between spawn attempts (0 = off)
    pub one_item_at_a_time: bool, // only ever one pickup on the field (projectiles don't count)
    pub pickup_reach: f32,        // pickup-box half-width: |dx| from the ECB anchor, front AND back
    pub pickup_r: f32, // legacy capsule radius: unused since the reach became an ECB box (kept for layout)
    pub spawn_iframes: i64, // respawn invulnerability window (frames)
    // knockback model (community / Project-M formula; see `knockback_units`). NOT the Melee decomp.
    pub knockback_mult: f32, // global launch-speed multiplier (>1 = everything flies further)
    pub kb_speed: f32,       // px/s per KB unit (turns formula units into launch velocity)
    pub kb_hitstun: f32,     // hitstun frames per KB unit (community 0.4: floor(0.4 * KB))
    pub tumble_speed: f32,   // launches faster than this knock down (or can be teched) on landing
    // item kinds
    pub laser: ItemConfig,
    pub bomb: ItemConfig,      // the red gun's arcing explosive (Bob-omb-ish)
    pub tetris: ItemConfig,    // the tetris gun's lob (its shot is an ink body, not a projectile)
    pub plasma: ItemConfig,    // the AC energy cannon's round (trigger data in ac.rs)
    pub throw_item: ThrowItem, // directional item-throw speeds + the armed item's contact hitbox
    // parity(v1-ink-global-tuning): the match owns the stroke registry, path budget, cursor reach, random spawn weight, and strike-to-unlock launch threshold consumed by every ink tool and body
    // drawn strokes / ink
    pub strokes: StrokeRegistry, // named stroke-material presets; row 0 = default (panel-editable)
    pub ink_budget: f32, // total path length (px) a fresh ink item can lay before it's spent
    pub ink_cursor_reach: f32, // CursorBrush: how far the drawing cursor floats off the body (px)
    pub ink_spawn_weight: f32, // relative spawn chance of a pen vs the guns (0 = never)
    pub badge_spawn_weight: f32, // relative spawn chance of a badge (0 = never random-spawns)
    pub ink_launch_speed: f32, // computed launch below this only shakes struck ink (no un-lock)
    // tech / knockdown / getup
    pub tech_window: i64, // frames a shield press stays valid as a tech before impact
    pub tech_intang: i64, // i-frames granted by a successful tech / getup
    pub techroll_speed: f32, // horizontal speed of a tech-roll / getup-roll
    pub techroll_frames: i64, // duration of a tech-roll / getup-roll
    pub knockdown_frames: i64, // floored lie time before you can act / auto-getup
    pub getup_frames: i64, // neutral getup rise duration (intangible)
    // stage restitution + the air-dodge save
    pub wall_bounce: f32,   // restitution when a LAUNCHED body hits a stage wall
    pub floor_bounce: f32,  // restitution when a fast LAUNCHED body hits the floor
    pub save_zero_pct: f32, // below this damage the save ZEROES launch vel (a full reset)
    pub save_scale: f32,    // launch vel kept per % over save_zero_pct (linear, clamped to 1)
    // clank / priority
    pub clank_diff: f32,     // damage gap (%) within which BOTH moves rebound
    pub rebound_frames: i64, // stagger length of a cancelled move
    pub rebound_push: f32,   // px/s the clank shoves each fighter apart
    // AC core + ship booster (match overlays)
    pub ac_spawn_weight: f32, // AC Core item spawn chance vs the other kinds
    pub booster_len: f32,     // exhaust reach past the rim (px)
    pub booster_cos: f32,     // sector half-angle as its cosine (0.86 ≈ ±30°)
    pub booster_kb: f32,      // launch speed at full throttle (px/s)
    pub booster_stun: i64,    // hitstun per blast pulse (also the re-blast cadence)
    // ship thrust (plans/body-unify.md step 6, "unpark the ship"): the exhaust's reaction impulse
    // on the hull itself, along `helm.aim` -- an acceleration (px/s^2), converted to ink-native
    // px/frame the same way `integrate_ink` converts gravity (`* DT * DT`). The hull's own
    // `gravity_scale` row is 0 (zero-g), so this is the ONLY force that ever moves it.
    pub ship_thrust_accel: f32,
    // wall cling (queue-2026-07-03 item 1; consumes Fighter::cling_used, resets on landing/kick)
    pub cling_frames: i64, // budget of frames a held wall cling can hang before it releases
    // ledge command grab (plans/ledge-domain.md, "ledges are ink command grabs"): the hand-anchor
    // catch against the lip projection. Panel-tunable so ledge feel iterates live.
    pub ledge_grab_r: f32, // hand-anchor grab radius (px): catch when |hand - lip| <= this
    pub ledge_fall_eps: f32, // must be falling at least this fast (px/s) to snap a ledge
    pub hand_reach_x: f32, // catching hand's forward reach from the feet (px)
    pub hand_rise: f32,    // catching hand's rise above the feet (px)
    // Melee-shaped directional grab zone (plans/ledge-ship-fixes.md #1): a box hanging
    // below+outboard of the lip, replacing the old hand-anchor circle. Feet-position gate, not
    // hand-anchor -- the box shape alone carries the "never grab from above" fix.
    pub ledge_ceil: f32, // max feet-above-lip that still catches (rising-from-under sliver)
    pub ledge_reach_down: f32, // zone depth below the lip (the hang-catch band)
    pub ledge_reach_x: f32, // outboard zone width
    pub ledge_lip_bite: f32, // inboard tolerance (turn-back-onto-edge)
    pub ledge_min_len: f32, // min ink segment length (px) to emit a grabbable lip
}

impl Default for MatchTune {
    fn default() -> Self {
        Self {
            smash_window: 5,    // flick + attack within 5f = smash (Melee's ~4f, a touch lenient)
            b_rev_window: 8,    // back-flick inside 8f of a special = B-reverse (PM ~2f is brutal)
            di_max_angle: 18.0, // ~18 deg of trajectory DI, the survival-DI ceiling
            coyote_frames: 9,   // walk off the lip and you keep your real jump for ~9f
            plat_drop_window: 3, // PM-ish: 3f to convert a platform Down tap into a Dtilt
            // Static: the fixed BLAST_* rect. (A 2026-07-04 experiment shipped InkExtends as
            // the default so a zone-material hull could carry the KO rect with the flying
            // ship; playtest verdict was "blast zone is wonky, lets maybe not extend blast
            // zone yet" -- the ship BOUNCES off the blast walls instead, see `integrate_ink`.)
            zone_mode: ZoneMode::Static,
            items_on: true,
            item_spawn_interval: 1200, // ~20s between spawns (one item at a time, so keep it rare)
            one_item_at_a_time: true,
            pickup_reach: 100.0, // pickup box: ~100px each way from the body (PM/Ult generous feel)
            pickup_r: 50.0, // legacy capsule radius (unused by the ECB-box reach; kept for layout)
            spawn_iframes: 120, // ~2s of respawn invulnerability
            knockback_mult: 1.4, // everything flies ~40% further (kills happen, kill moves matter)
            kb_speed: 6.0,  // KB units -> px/s (tuned so kill moves send ~kill distance)
            kb_hitstun: 0.4, // community/PM constant: hitstun = floor(0.4 * KB)
            tumble_speed: 620.0, // ~a mid-% launch; below this you just land on your feet
            laser: ItemConfig::LASER,
            bomb: ItemConfig::BOMB,
            tetris: ItemConfig::TETRIS,
            plasma: ItemConfig::PLASMA,
            throw_item: ThrowItem::DEFAULT,
            strokes: StrokeRegistry::DEFAULT,
            ink_budget: 1.0e9, // effectively infinite ink while testing (was 900.0 ~ stage width)
            ink_cursor_reach: 140.0,
            ink_spawn_weight: 0.6,
            badge_spawn_weight: 0.35, // rare-ish: a badge permanently mods a character
            ink_launch_speed: 240.0,  // lasers chip+shake (~115 px/s); throws/bombs launch (500+)
            tech_window: 20,
            tech_intang: 26,
            techroll_speed: vel(2.4), // a touch faster than a roll (roll_speed ~1.8)
            techroll_frames: 24,
            knockdown_frames: 40,
            getup_frames: 24,
            wall_bounce: 0.45, // launched into the stage wall, a tumbling body kicks back off it
            floor_bounce: 0.55, // spiked into the floor, a fast tumbling body bounces back up
            save_zero_pct: 100.0, // under 100% the save is a full momentum reset
            save_scale: 0.005, // 0.5% of launch vel kept per % over; full knockback again at 300%
            clank_diff: 9.0,   // Smash rule: within 9% both cancel; past it the big move wins
            rebound_frames: 18,
            rebound_push: vel(0.8),
            ac_spawn_weight: 0.35,
            booster_len: 150.0,
            booster_cos: 0.86,    // ±30° sector
            booster_kb: vel(2.6), // walljump-kick class shove at full throttle
            booster_stun: 20,
            // a big, heavy-feeling ship: ~1s of sustained thrust reaches roughly a fighter's walk
            // speed (v = a*t at this DT*DT conversion), tune freely.
            ship_thrust_accel: 400.0,
            cling_frames: 90, // 1.5s @ 60Hz
            // defaults preserve the old stage-lip feel: reach 35 + rise 35 center the hand in the
            // former 70px-wide / -20..+90 y snap window, radius 66 covers its corners, eps 150
            // unchanged (the old `LEDGE_FALL_EPS`).
            ledge_grab_r: 66.0,
            ledge_fall_eps: 150.0,
            hand_reach_x: 35.0,
            hand_rise: 35.0,
            // Melee-shaped directional grab zone (plans/ledge-ship-fixes.md #1): a box hanging
            // below+outboard of the lip. `ledge_ceil` small so a body above the lip never
            // catches (the turbo-glue fix); `ledge_reach_down` ~= the old y-window's +90 half.
            ledge_ceil: 8.0,
            ledge_reach_down: 90.0,
            ledge_reach_x: 35.0,
            ledge_lip_bite: 12.0,
            ledge_min_len: 60.0,
        }
    }
}

/// Roster length: the number of DISTINCT physics kits (Knee Man, Falcon, Lucas today). This is not
/// the shell roster length -- `Tune` embeds `[CharSpec; ROSTER_N]` and is copied by value per fighter
/// every `step`, so padding it with duplicate kits per art slot is expensive (it once overflowed the
/// test stack). Instead a `char_id` (a SHELL art slot) is mapped onto one of these rows by
/// `art_slot_row` before indexing -- see `chars::ART_SLOT_ROW`. Grows only when a genuinely new kit
/// lands (plans/mod-api.md tier-1 file loading), never per art slot.
pub const ROSTER_N: usize = 3;

/// The per-character mod rows a `Tune` carries. Both peers verify identical rosters over the
/// handshake hash (plans/mod-api.md): a mod that changes a `CharSpec` changes this.
/// The character library, a SHARED handle. `Tune` used to embed `[CharSpec; ROSTER_N]` by value, so
/// every per-fighter `for_char` copy (every frame, `lib.rs`) dragged the whole roster -- ~1.65 MB/frame
/// projected at 70 characters, which overflows the stack. `Arc<[CharSpec]>` makes a `Tune` clone a
/// pointer bump instead, so the library can be any length at flat per-frame cost. Cloning a `Tune`
/// shares the same rows (rollback/`for_char` never mutate them); the tuning panel / tests that DO edit
/// a row use `Arc::make_mut` to copy-on-write. `ROSTER_N` is now just the built-in count.
pub type Roster = std::sync::Arc<[CharSpec]>;

/// Live "feel" config in PIXEL SPACE: the flat view a single fighter resolves to (every number the
/// FSM / physics / combat reads), the `roster` of character rows it can resolve, and the match
/// config it was built from. `step` calls `for_char(char_id)` per fighter, so physics + moveset
/// follow that fighter's `CharSpec` row while the match knobs stay shared. This is the ggrs `Config`
/// + handshake payload + egui-panel edit target; the flat char fields are roster row 0's resolved
/// view and stay panel-editable.
// NOT Copy: `roster: Arc<[CharSpec]>` is a shared handle (see `Roster`). `Clone` is a pointer bump.
#[derive(Clone, Serialize, Deserialize)]
// sprefa:paths -- elects this struct for `.dl/gen-tune-paths.dl` (generates core/src/tune_paths.rs)
pub struct Tune {
    pub gravity: f32,
    pub max_fall: f32,
    pub fastfall: f32,
    pub walk_speed: f32,
    pub dash_init: f32,
    pub run_speed: f32,
    pub ground_accel: f32,
    pub ground_friction: f32,
    pub fullhop_v: f32,  // negative
    pub shorthop_v: f32, // negative
    pub airjump_v: f32,  // negative
    pub airjump_h: f32,
    pub jump_h_init: f32,
    pub jump_h_max: f32,
    pub air_speed: f32,
    pub air_accel: f32,
    pub air_friction: f32,
    pub momentum_carry: f32,
    pub roll_speed: f32,
    pub airdodge_speed: f32,
    pub airdodge_drag: f32,
    pub ledgejump_v: f32, // negative
    pub max_air_jumps: i64,
    pub max_air_dodges: i64,
    pub jumpsquat: i64,
    pub landing_lag: i64,
    pub dash_window: i64,
    pub pivot_frames: i64,
    pub dash_turn_accel: f32,
    pub dashstop_friction: f32,
    pub spotdodge_frames: i64,
    pub roll_frames: i64,
    pub airdodge_frames: i64,
    pub ledge_intang: i64,
    pub climb_frames: i64,
    pub buffer_frames: i64,
    pub jab: AttackData,
    pub nair: AttackData,
    pub fair: AttackData,
    pub bair: AttackData,
    pub uair: AttackData,
    pub dair: AttackData,
    pub dtilt: AttackData,
    pub ftilt: AttackData,
    pub utilt: AttackData,
    pub fsmash: AttackData,
    pub usmash: AttackData,
    pub dsmash: AttackData,
    pub dash_attack: AttackData,
    pub dair_threshold: f32, // |aim_y| past this (and steeper than horizontal) picks dair/uair
    pub smash_window: i64, // attack within this many frames of a hard stick flick = smash, not tilt
    pub b_rev_window: i64, // frames into a special where a back-flick still B-reverses it
    pub autohop_dmg: f32,  // damage multiplier for auto-short-hop aerials (jump+attack macro)
    pub di_max_angle: f32, // max degrees the victim's stick can rotate a launch trajectory (survival DI)
    pub coyote_frames: i64, // grace window after walking off an edge to still get a full grounded jump
    pub plat_drop_window: i64, // soft-platform drop tilt-window: frames a Down tap waits before
    // dropping, so a Down+Attack inside it reads as a Dtilt. 1 = instant
    // drop / frame-perfect tilt (Melee); larger = more lenient tilt (PM).
    pub specials: [SpecialMove; 4], // B-move loadout, indexed by special_slot (N/Side/Up/Down)
    // items (match settings, not character-derived)
    pub items_on: bool,           // master switch for item spawns
    pub item_spawn_interval: i64, // frames between spawn attempts (0 = off)
    pub one_item_at_a_time: bool, // only ever one pickup on the field (projectiles don't count)
    pub pickup_reach: f32,        // pickup-box half-width: |dx| from the ECB anchor, front AND back
    pub pickup_r: f32, // legacy capsule radius: unused since the reach became an ECB box (kept for layout)
    pub spawn_iframes: i64, // respawn invulnerability window (frames)
    pub knockback_mult: f32, // global launch-speed multiplier (>1 = everything flies further)
    pub zone_mode: ZoneMode, // how the fighter blast zone is computed each frame (menu-toggleable)
    // knockback model (community / Project-M formula; see `knockback_units`). NOT the Melee decomp.
    pub weight: f32, // victim weight in the KB formula (Falcon/KneeMan-ish ~104)
    pub zone_exempt: bool, // this character is never KO'd by the blast zone (config, not sim state)
    pub kb_speed: f32, // px/s per KB unit (turns formula units into launch velocity)
    pub kb_hitstun: f32, // hitstun frames per KB unit (community 0.4: floor(0.4 * KB))
    pub laser: ItemConfig,
    pub bomb: ItemConfig,      // the red gun's arcing explosive (Bob-omb-ish)
    pub tetris: ItemConfig,    // the tetris gun's lob (its shot is an ink body, not a projectile)
    pub throw_item: ThrowItem, // directional item-throw speeds + the armed item's contact hitbox
    // drawn stroke paths
    pub strokes: StrokeRegistry, // named stroke-material presets; row 0 = default (panel-editable)
    pub ink_budget: f32, // total path length (px) a fresh ink item can lay before it's spent
    pub ink_cursor_reach: f32, // CursorBrush: how far the drawing cursor floats off the body (px)
    pub ink_spawn_weight: f32, // relative spawn chance of a pen vs the guns (0 = never random-spawns)
    pub badge_spawn_weight: f32, // relative spawn chance of a badge (0 = never random-spawns)
    pub ink_launch_speed: f32, // computed launch px/s below this only shakes struck ink (no un-lock)
    pub fastfall_threshold: f32, // stick aim_y must reach this (and beat |dir|) to fast fall
    // grab -> pummel -> throw
    pub grab_startup: i64,      // wind-up before the grab reach turns on
    pub grab_active: i64,       // frames the grab can catch
    pub grab_recovery: i64,     // whiff cool-down (heavy: a missed grab is punishable)
    pub grab_range: f32,        // forward reach of the grab from the body
    pub grab_hold: i64,         // auto-release countdown once a hold lands
    pub grab_mash: i64,         // extra countdown removed per fresh victim input (mash to escape)
    pub pummel_damage: f32,     // damage per pummel tap while holding
    pub pummel_bonus: i64,      // hold extended per pummel (capped at grab_hold)
    pub throws: [ThrowData; 4], // fwd / back / up / down
    // tech / knockdown / getup
    pub tumble_speed: f32, // launches faster than this knock down (or can be teched) on landing
    pub tech_window: i64,  // frames a shield press stays valid as a tech before impact
    pub tech_intang: i64,  // i-frames granted by a successful tech / getup
    pub techroll_speed: f32, // horizontal speed of a tech-roll / getup-roll
    pub techroll_frames: i64, // duration of a tech-roll / getup-roll
    pub knockdown_frames: i64, // floored lie time before you can act / auto-getup
    pub getup_frames: i64, // neutral getup rise duration (intangible)
    // stage geometry
    pub wall_bounce: f32, // restitution when a LAUNCHED (tumbling) body hits a stage wall;
    // 0 = dead stop (normal recovery), >0 = bounce off (geo::reflect)
    pub floor_bounce: f32, // restitution when a fast LAUNCHED (tumbling) body hits the floor;
    // 0 = dead stop (land), >0 = bounce up (dair spike -> funny bounce)
    // the save: a buffered shield press fires an air dodge the frame hitstun expires,
    // consuming an air-dodge charge and cancelling launch momentum ("literally air
    // dodge out of stun"). Cancel strength scales with damage, Ultimate-style:
    pub save_zero_pct: f32, // below this damage the save ZEROES launch vel (a full reset)
    pub save_scale: f32,    // launch vel kept per % over save_zero_pct (linear, clamped to 1)
    // shield blocking (the block itself resolves in `strike` via Guard::Shield)
    pub shield_max: f32, // full shield health: total % it can eat before breaking
    pub shield_regen: f32, // hp per frame recovered while the guard is down
    pub shield_decay: f32, // hp per frame drained while the guard is held up
    pub shieldstun_per_dmg: f32, // frames locked in shield per % blocked (+2 base)
    pub shield_push: f32, // px/s pushback per % blocked (slides the blocker out)
    pub shieldbreak_frames: i64, // dizzy length after a break (fully punishable)
    // smash charge (the pin lives in the smash arms; payoff in `charge_mult`)
    pub charge_max: i64, // max frames a smash can bank at the pin
    pub charge_dmg: f32, // damage/kb multiplier at a full bank (linear from 1.0)
    // getup / ledge attacks
    pub ledge_attack: AttackData, // ledge getup attack (intangible through startup)
    pub getup_attack: AttackData, // knockdown getup attack: front-then-back sweep
    // clank / priority (hitbox-vs-hitbox; transcendent boxes never clank)
    pub clank_diff: f32,     // damage gap (%) within which BOTH moves rebound
    pub rebound_frames: i64, // stagger length of a cancelled move
    pub rebound_push: f32,   // px/s the clank shoves each fighter apart
    // walljump (consumes the wall_touch window; no air jump spent)
    pub walljump_v: f32, // vertical kick off a wall (negative = up)
    pub walljump_h: f32, // horizontal kick away from the wall
    // footstool (Act::Footstool: jump press directly above a body)
    pub footstool_v: f32,     // the jumper's pop off a head (negative = up)
    pub footstool_spike: f32, // downward shove on an airborne victim (positive = down)
    pub footstool_stun: i64,  // victim stagger frames (grounded victims take half)
    // crawl
    pub crawl_speed: f32, // crouched creep speed (well under walk)
    // Armored Core overlay (Badge::AcCore; ac.rs). Weapon trigger data is `ac::arm_spec`
    // consts; these are the FRAME: how the mech moves and how often the core drops.
    pub ac_spawn_weight: f32, // AC Core item spawn chance vs the other kinds
    pub ac_grav_mult: f32,    // extreme gravity: multiplier on airborne gravity while AC'd
    pub ac_boost_accel: f32,  // held-jump thrust accel toward the stick (px/s^2)
    pub ac_boost_max: f32,    // speed cap the boost thrusts toward (px/s)
    pub ac_qb_speed: f32,     // quick-boost burst speed (jump tap, px/s)
    pub ac_qb_cd: i64,        // quick-boost cooldown (frames)
    pub plasma: ItemConfig,   // the energy cannon's round (hit row; trigger data in ac.rs)
    // ship booster (the parked hull's engine exhaust; plans/lovers-ship.md)
    pub booster_len: f32,       // exhaust reach past the rim (px)
    pub booster_cos: f32,       // sector half-angle as its cosine (0.86 ≈ ±30°)
    pub booster_kb: f32,        // launch speed at full throttle (px/s)
    pub booster_stun: i64,      // hitstun per blast pulse (also the re-blast cadence)
    pub ship_thrust_accel: f32, // the hull's own thrust reaction accel along helm.aim (px/s^2)
    // wall cling (queue-2026-07-03 item 1; consumes Fighter::cling_used, resets on landing/kick)
    pub cling_frames: i64, // budget of frames a held wall cling can hang before it releases
    // ledge command grab (plans/ledge-domain.md): hand-anchor catch knobs, panel-tunable.
    pub ledge_grab_r: f32,     // hand-anchor grab radius (px)
    pub ledge_fall_eps: f32,   // min fall speed to snap a ledge (px/s)
    pub hand_reach_x: f32,     // catching hand's forward reach from the feet (px)
    pub hand_rise: f32,        // catching hand's rise above the feet (px)
    pub ledge_ceil: f32,       // max feet-above-lip that still catches (rising-from-under sliver)
    pub ledge_reach_down: f32, // zone depth below the lip
    pub ledge_reach_x: f32,    // outboard zone width
    pub ledge_lip_bite: f32,   // inboard tolerance (turn-back-onto-edge)
    pub ledge_min_len: f32,    // min ink segment length (px) to emit a grabbable lip
    /// The character rows this config can resolve, indexed by `Fighter::char_id`. Row 0 is the
    /// character the flat fields above are the resolved view of (what the panel edits live).
    pub roster: Roster,
}

impl Tune {
    /// Resolve one character's `CharSpec` against a `MatchTune` into the flat pixel view the sim
    /// reads. The physics + kit fields convert from source units (units/frame @ 60fps -> px/s);
    /// moves + specials + match knobs copy through. The `roster` is filled with `spec` in every row;
    /// callers that want a real multi-character roster overwrite it afterward (see `for_char`).
    pub fn resolve(spec: &CharSpec, m: &MatchTune) -> Self {
        let c = &spec.phys;
        Self {
            // physics (CharData, source -> pixel)
            gravity: acc(c.gravity),
            max_fall: vel(c.max_fall),
            fastfall: vel(c.fastfall),
            walk_speed: vel(c.walk_max),
            dash_init: vel(c.dash_init),
            run_speed: vel(c.run_max),
            ground_accel: acc(c.ground_accel),
            ground_friction: acc(c.ground_friction),
            fullhop_v: -vel(c.fullhop_v),
            shorthop_v: -vel(c.shorthop_v),
            airjump_v: -vel(c.airjump_v),
            airjump_h: vel(c.airjump_h),
            jump_h_init: vel(c.jump_h_init),
            jump_h_max: vel(c.jump_h_max),
            air_speed: vel(c.air_speed),
            air_accel: acc(c.air_accel),
            air_friction: acc(c.air_friction),
            momentum_carry: c.momentum_carry,
            roll_speed: vel(c.roll_speed),
            airdodge_speed: vel(c.airdodge_speed),
            airdodge_drag: acc(c.airdodge_drag),
            ledgejump_v: -vel(c.ledgejump_v),
            max_air_jumps: c.max_air_jumps as i64,
            max_air_dodges: c.max_air_dodges as i64,
            jumpsquat: c.jumpsquat,
            landing_lag: c.landing_lag,
            dash_window: c.dash_window,
            pivot_frames: c.pivot_frames,
            dash_turn_accel: acc(c.dash_turn_accel), // an acceleration, like ground_accel
            dashstop_friction: acc(c.dashstop_friction),
            spotdodge_frames: c.spotdodge_frames,
            roll_frames: c.roll_frames,
            airdodge_frames: c.airdodge_frames,
            ledge_intang: c.ledge_intang,
            climb_frames: c.climb_frames,
            buffer_frames: c.buffer_frames,
            // moves (character, direct)
            jab: spec.jab,
            nair: spec.nair,
            fair: spec.fair,
            bair: spec.bair,
            uair: spec.uair,
            dair: spec.dair,
            dtilt: spec.dtilt,
            ftilt: spec.ftilt,
            utilt: spec.utilt,
            fsmash: spec.fsmash,
            usmash: spec.usmash,
            dsmash: spec.dsmash,
            dash_attack: spec.dash_attack,
            ledge_attack: spec.ledge_attack,
            getup_attack: spec.getup_attack,
            specials: spec.specials,
            throws: spec.throws,
            // aerial thresholds (character)
            dair_threshold: spec.dair_threshold,
            autohop_dmg: spec.autohop_dmg,
            fastfall_threshold: spec.fastfall_threshold,
            // grab kit (character)
            grab_startup: spec.grab_startup,
            grab_active: spec.grab_active,
            grab_recovery: spec.grab_recovery,
            grab_range: spec.grab_range,
            grab_hold: spec.grab_hold,
            grab_mash: spec.grab_mash,
            pummel_damage: spec.pummel_damage,
            pummel_bonus: spec.pummel_bonus,
            weight: spec.weight,
            zone_exempt: spec.zone_exempt,
            // smash charge (character)
            charge_max: spec.charge_max,
            charge_dmg: spec.charge_dmg,
            // shield (character)
            shield_max: spec.shield_max,
            shield_regen: spec.shield_regen,
            shield_decay: spec.shield_decay,
            shieldstun_per_dmg: spec.shieldstun_per_dmg,
            shield_push: spec.shield_push,
            shieldbreak_frames: spec.shieldbreak_frames,
            // walljump / footstool / crawl (character, source -> pixel)
            walljump_v: -vel(spec.walljump_v),
            walljump_h: vel(spec.walljump_h),
            footstool_v: -vel(spec.footstool_v),
            footstool_spike: vel(spec.footstool_spike),
            footstool_stun: spec.footstool_stun,
            crawl_speed: vel(spec.crawl_speed),
            // AC frame (character; some source -> pixel)
            ac_grav_mult: spec.ac_grav_mult,
            ac_boost_accel: acc(spec.ac_boost_accel),
            ac_boost_max: vel(spec.ac_boost_max),
            ac_qb_speed: vel(spec.ac_qb_speed),
            ac_qb_cd: spec.ac_qb_cd,
            // match-global feel
            smash_window: m.smash_window,
            b_rev_window: m.b_rev_window,
            di_max_angle: m.di_max_angle,
            coyote_frames: m.coyote_frames,
            plat_drop_window: m.plat_drop_window,
            zone_mode: m.zone_mode,
            // match-global items
            items_on: m.items_on,
            item_spawn_interval: m.item_spawn_interval,
            one_item_at_a_time: m.one_item_at_a_time,
            pickup_reach: m.pickup_reach,
            pickup_r: m.pickup_r,
            spawn_iframes: m.spawn_iframes,
            knockback_mult: m.knockback_mult,
            kb_speed: m.kb_speed,
            kb_hitstun: m.kb_hitstun,
            tumble_speed: m.tumble_speed,
            laser: m.laser,
            bomb: m.bomb,
            tetris: m.tetris,
            plasma: m.plasma,
            throw_item: m.throw_item,
            strokes: m.strokes,
            ink_budget: m.ink_budget,
            ink_cursor_reach: m.ink_cursor_reach,
            ink_spawn_weight: m.ink_spawn_weight,
            badge_spawn_weight: m.badge_spawn_weight,
            ink_launch_speed: m.ink_launch_speed,
            tech_window: m.tech_window,
            tech_intang: m.tech_intang,
            techroll_speed: m.techroll_speed,
            techroll_frames: m.techroll_frames,
            knockdown_frames: m.knockdown_frames,
            getup_frames: m.getup_frames,
            wall_bounce: m.wall_bounce,
            floor_bounce: m.floor_bounce,
            save_zero_pct: m.save_zero_pct,
            save_scale: m.save_scale,
            clank_diff: m.clank_diff,
            rebound_frames: m.rebound_frames,
            rebound_push: m.rebound_push,
            ac_spawn_weight: m.ac_spawn_weight,
            booster_len: m.booster_len,
            booster_cos: m.booster_cos,
            booster_kb: m.booster_kb,
            booster_stun: m.booster_stun,
            ship_thrust_accel: m.ship_thrust_accel,
            cling_frames: m.cling_frames,
            ledge_grab_r: m.ledge_grab_r,
            ledge_fall_eps: m.ledge_fall_eps,
            hand_reach_x: m.hand_reach_x,
            hand_rise: m.hand_rise,
            ledge_ceil: m.ledge_ceil,
            ledge_reach_down: m.ledge_reach_down,
            ledge_reach_x: m.ledge_reach_x,
            ledge_lip_bite: m.ledge_lip_bite,
            ledge_min_len: m.ledge_min_len,
            roster: std::sync::Arc::from([*spec; ROSTER_N]),
        }
    }

    /// Read the match-global knobs back out of the flat view. Trivial passthrough (the flat fields
    /// ARE pixel-space), so panel edits to any match knob carry into a per-character re-resolve.
    pub fn match_tune(&self) -> MatchTune {
        MatchTune {
            smash_window: self.smash_window,
            b_rev_window: self.b_rev_window,
            di_max_angle: self.di_max_angle,
            coyote_frames: self.coyote_frames,
            plat_drop_window: self.plat_drop_window,
            zone_mode: self.zone_mode,
            items_on: self.items_on,
            item_spawn_interval: self.item_spawn_interval,
            one_item_at_a_time: self.one_item_at_a_time,
            pickup_reach: self.pickup_reach,
            pickup_r: self.pickup_r,
            spawn_iframes: self.spawn_iframes,
            knockback_mult: self.knockback_mult,
            kb_speed: self.kb_speed,
            kb_hitstun: self.kb_hitstun,
            tumble_speed: self.tumble_speed,
            laser: self.laser,
            bomb: self.bomb,
            tetris: self.tetris,
            plasma: self.plasma,
            throw_item: self.throw_item,
            strokes: self.strokes,
            ink_budget: self.ink_budget,
            ink_cursor_reach: self.ink_cursor_reach,
            ink_spawn_weight: self.ink_spawn_weight,
            badge_spawn_weight: self.badge_spawn_weight,
            ink_launch_speed: self.ink_launch_speed,
            tech_window: self.tech_window,
            tech_intang: self.tech_intang,
            techroll_speed: self.techroll_speed,
            techroll_frames: self.techroll_frames,
            knockdown_frames: self.knockdown_frames,
            getup_frames: self.getup_frames,
            wall_bounce: self.wall_bounce,
            floor_bounce: self.floor_bounce,
            save_zero_pct: self.save_zero_pct,
            save_scale: self.save_scale,
            clank_diff: self.clank_diff,
            rebound_frames: self.rebound_frames,
            rebound_push: self.rebound_push,
            ac_spawn_weight: self.ac_spawn_weight,
            booster_len: self.booster_len,
            booster_cos: self.booster_cos,
            booster_kb: self.booster_kb,
            booster_stun: self.booster_stun,
            ship_thrust_accel: self.ship_thrust_accel,
            cling_frames: self.cling_frames,
            ledge_grab_r: self.ledge_grab_r,
            ledge_fall_eps: self.ledge_fall_eps,
            hand_reach_x: self.hand_reach_x,
            hand_rise: self.hand_rise,
            ledge_ceil: self.ledge_ceil,
            ledge_reach_down: self.ledge_reach_down,
            ledge_reach_x: self.ledge_reach_x,
            ledge_lip_bite: self.ledge_lip_bite,
            ledge_min_len: self.ledge_min_len,
        }
    }

    /// The per-fighter view: physics + moveset for shell art slot `char_id`, the match knobs shared.
    /// `char_id` is a shell art slot, mapped onto a distinct-kit row by `chars::art_slot_row` (see
    /// `chars::ART_SLOT_ROW`); an out-of-range slot maps to row 0 (never panics). Row 0 is the flat
    /// view itself (so the panel keeps live-editing that character); any other row re-resolves its
    /// `CharSpec` against this config's live match knobs.
    pub fn for_char(&self, char_id: u8) -> Tune {
        let idx = crate::v1::chars::art_slot_row(char_id);
        if idx == 0 {
            return self.clone();
        }
        let mut t = Tune::resolve(&self.roster[idx], &self.match_tune());
        t.roster = self.roster.clone(); // Arc bump, not a roster copy
        t
    }

    /// A single character's weight (the KB formula's `w`) straight off its roster row, no full
    /// re-resolve. Row 0 reads the flat field (the panel-editable view); other rows read the
    /// `CharSpec` directly. Lets the strike ritual charge the victim's own weight from whatever
    /// config it was handed. `char_id` (a shell art slot) maps onto a row via `chars::art_slot_row`.
    pub fn weight_of(&self, char_id: u8) -> f32 {
        let idx = crate::v1::chars::art_slot_row(char_id);
        if idx == 0 {
            self.weight
        } else {
            self.roster[idx].weight
        }
    }

    /// Build a flat view from a bare `CharData` (KNEEMAN's kit + default match rules). Kept for the
    /// tests and the panel "reset feel" path that start from a `CharData`.
    pub fn from_char(c: &CharData) -> Self {
        Self::resolve(&CharSpec::from_data(c), &MatchTune::default())
    }
}

impl Default for Tune {
    /// Row 0 is the flat KNEEMAN view; row 1 (the zombie, `char_id` 1 / P2's default) is the
    /// FALCON row -- KNEEMAN's kit with the up-B command grab (`CharSpec::falcon`). This is the
    /// "drop it into a roster row" arming the falcon() doc promises: until 2026-07-04 no live
    /// row carried it, so the grab existed only in tests ("grabboxes aint grab"). The roster
    /// itself comes from `chars::roster()` (plans/swordsman-lucas.md row 1: the char registry).
    fn default() -> Self {
        let roster = crate::v1::chars::roster();
        let mut t = Self::resolve(&roster[0], &MatchTune::default());
        t.roster = std::sync::Arc::from(roster);
        t
    }
}
