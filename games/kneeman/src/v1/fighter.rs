//! One fighter as a plain value: the `Fighter` struct + its impl (spawn, badge/state-name
//! accessors, the input-buffer lane helpers), plus the post-KO `respawn`. Split out of
//! `lib.rs` (R5); struct field order is PRESERVED EXACTLY (positional bincode).

use crate::v1::body::SurfOwner;
use crate::v1::state::{Action, CharState, Lane, N_LANE, Slot};
use crate::v1::{Badge, MAX_HB, MAX_PLAYERS, N_BADGE_BITS, SHIELD_MAX, Tune, Vector2};
use serde::{Deserialize, Serialize};

/// One fighter as a plain value. Two of these make a `SimState`. Everything here is
/// per-fighter (the old single-player SimState fields); `damage`/`hitstun` were the old
/// `dummy_*` fields, now owned by every fighter (each can take and deal hits).
/// `frame` is the per-fighter STATE timer (reset on every transition), not a global clock.
#[derive(Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fighter {
    pub frame: i64,
    pub pos: Vector2,
    pub vel: Vector2,
    pub state: CharState,
    pub facing: f32,    // +1 right, -1 left
    pub air_jumps: u8,  // remaining air jumps (refreshed on ground/ledge contact)
    pub air_dodges: u8, // remaining air dodges
    pub fast_falling: bool,
    pub full_hop: bool, // decided during JumpSquat (jump still held at takeoff)
    pub buf: [Slot; N_LANE], // input buffer, one lane each (Movement/Aerial/Attack/Grab); see Lane
    pub autohop_aerial: bool, // current aerial came from the jump+attack auto-short-hop (reduced dmg)
    pub intangible: bool,     // dodge / ledge i-frames (drives the debug color)
    pub regrab_lock: i64,     // frames before a ledge can be re-grabbed
    pub ground_plat: i32,     // index into PLATFORMS the fighter stands on (-1 = airborne)
    pub ground_ink: i8, // index into SimState.paths when standing on drawn ink (-1 = not on ink)
    // Per-hitbox, per-victim re-hit countdown: `hit_cd[box][victim] > 0` means that box of THIS
    // fighter's current move can't hit that victim yet (it just connected, or is mid-window).
    // Shapes with equal hitbox IDs receive the same cooldown on contact. A box
    // re-arms after its `refresh`; a fresh swing zeroes the whole grid (`arm_hits`). Replaces the old
    // `attack_hit: bool` so a 3-box jab combo / multi-hit stomp each land their own sequenced hits,
    // and a wide box hits every overlapping victim once.
    pub hit_cd: [[i16; MAX_PLAYERS]; MAX_HB],
    pub hitlag: i64,     // impact freeze on connect (this fighter held)
    pub damage: f32,     // accumulated % (knockback scales with this)
    pub hitstun: i64,    // frames launched/can't act (drives the hit flash + knockback slide)
    pub holding: i8,     // index into SimState.items of the held item, or -1 (empty-handed)
    pub coyote: u8,      // grace frames after walking off an edge where jump = full grounded jump
    pub invuln: u8,      // spawn/respawn i-frames: ignore incoming hits while > 0
    pub grab_link: i8,   // grab partner index (victim if GrabHold, grabber if Grabbed), else -1
    pub grab_timer: i64, // hold countdown: ticks down + victim mash chips it; <= 0 = break free
    pub tech_buf: u8,    // tech window: a shield press during hitstun arms a tech for N frames
    pub tumble: bool,    // this launch is hard enough to knock down (or be teched) on landing
    pub wall_hit: i64, // frames left in the wall-bounce tilt window (cosmetic: shell tilts + swaps clip)
    pub drop_buf: i64, // soft-platform drop tilt-window: a Down tap arms it; an attack in-window
    // converts to Dtilt + cancels; expires -> drop through. 0 = no pending drop.
    // smash-input flick memory: frames since the stick last crossed into the hard zone (255 = stale).
    // An attack press while the matching age is fresh reads as a smash, not a tilt — the classic
    // flick-vs-hold split that works on both analog sticks and digital keyboards.
    pub flick_x_age: u8,        // horizontal flick age
    pub flick_y_age: u8,        // vertical flick age (sign read off the live stick at the press)
    pub stick_was_hard_x: bool, // |dir| was past DASH_THRESH last frame (edge detector state)
    pub stick_was_hard_y: bool, // |aim_y| was past DASH_THRESH last frame
    pub cstick_held: bool, // c-stick was deflected last frame (edge detector for Strong/Aerial)
    pub b_reversed: bool,  // this special already spent its one B-reverse (reset on entry)
    /// The attack press that picked an item up is still down: suppress Fire/Draw until it is
    /// released, so grabbing a gun off the ground never auto-shoots (Smash pickup feel).
    pub pickup_hold: bool,
    pub shield_hp: f32, // shield health: drains while up / on a block, regens while down; 0 = break
    pub shield_stun: i64, // frames locked in Shield after a block (guard hitstun)
    pub charge: i64,    // smash-charge frames banked while the swing's clock is pinned
    pub wall_touch: u8, // frames left in the walljump window after an airborne wall contact
    pub wall_nx: f32,   // outward normal x of that wall (+1 = wall on the left, kick right)
    /// Attached badges (bitmask of `Badge` bits): passive character mods from badge item
    /// pickups. No hand slot, no drop, no throw; respawns keep them.
    pub badges: u8,
    pub arm: u8, // AC arm weapon id (`ac::ArmWeapon`); meaningless without Badge::AcCore
    pub arm_cd: u8, // frames until the arm gun can fire again (set by ac_fire, ticked in FSM)
    pub qb_cd: u8, // frames until the next quick boost (jump tap while AC'd)
    /// WHICH character this fighter is — sim state, not shell cosmetics: it rolls back,
    /// folds into the checksum, and respawn keeps it. The shell paints sprites FROM this
    /// (never the other way), so in-map character-switch mechanics just mutate it here.
    /// Menu picks enter via the state cell offline / a session-start stamp in netplay.
    pub char_id: u8,
    /// Air-catch latch: frames left in which a thrown in-flight item touching this fighter is
    /// caught instead of hitting. Armed by an airborne, empty-handed GRAB press (za_warudo's
    /// input scan, `CATCH_WINDOW` frames) so the catch isn't frame-perfect; ticked with the
    /// other per-frame timers; zeroed by a successful catch so one press can't catch twice.
    /// Appended at the struct's END: bincode is positional (never reorder).
    pub catch_win: i64,
    /// AC fuel meter (plans/ac-ship-backlog.md item 1): frames of mech wear left, armed to
    /// `ac::AC_GAS_FRAMES` on `Badge::AcCore` attach (`acts::attach_badge`), ticked down once
    /// per active frame in `fighters::tick_badge_meters`, and at 0 clears only the `AcCore`
    /// bit -- the only way out of the armored core today. 0 = no badge / expired; meaningless
    /// otherwise. Also zeroed on respawn (the mech doesn't ride a KO). Appended at the
    /// struct's END: bincode is positional (never reorder/insert mid-struct).
    pub ac_gas: i64,
    /// Per-badge wear timer (queue-2026-07-03 item 6): frames left before badge bit `i` auto-
    /// clears, indexed by that bit's position in `badges` (`fighters::wear::bit_index`). Armed
    /// on attach by `acts::attach_badge` (default `fighters::wear`'s 600-frame timeout;
    /// `AcCore` opts out and keeps its own `ac_gas` meter), ticked down in
    /// `fighters::tick_badge_meters`. 0 = no wear armed for that bit. Zeroed on respawn along
    /// with every other badge field. Appended at the struct's END: bincode is positional
    /// (never reorder/insert mid-struct).
    pub badge_gas: [i64; N_BADGE_BITS],
    /// Wall-cling budget spent this airtime (queue-2026-07-03 item 1): frames already held
    /// pressing INTO an armed wall contact, vs `Tune::cling_frames`. Reset to 0 on any floor
    /// landing and on a walljump kick-off (jump-press or buttonless) -- NOT on the wall touch
    /// itself, or the cap would never bind while just leaning on the wall. 0 = full budget.
    /// Appended at the struct's END: bincode is positional (never reorder/insert mid-struct).
    pub cling_used: i64,
    /// Falcon up-B command grab (queue-2026-07-03 item 4): this fighter is the GRABBER in a
    /// command-grab latch, not a normal grab-hold. Set true when the stationary hug (`SpecialU` +
    /// `SpecialKind::DiveGrab`) catches; while true the shared `GrabHold`/`Grabbed` pair does NOT
    /// pummel/throw/mash -- `resolve_grab` counts `grab_timer` down to a fixed-angle explosion
    /// instead. Cleared on the boom and on any release. Meaningless off a command latch. Appended
    /// at the struct's END: bincode is positional (never reorder/insert mid-struct).
    pub dive_latch: bool,
    /// Occupied ship station (plans/lovers-ship.md "v2: stations"): the `SHIP_STATIONS` anchor index
    /// this fighter is LOCKED to, or -1 = free. While `>= 0` the FSM is frozen and `pos` follows the
    /// anchor each frame (the same shape as `Grabbed` following `grab_link`); jump releases. Set on an
    /// occupy interact (`Act::Occupy`), cleared on jump / launch / KO (respawn's fresh spawn zeroes
    /// it). Appended at the struct's END: bincode is positional (never reorder/insert mid-struct).
    pub station: i8,
    /// Ink slot of the wall behind the armed `wall_touch` contact (-1 = a platform/stage wall).
    /// The wall-cling ride carry reads it: clinging to a MOVING hull must drag the clinger along
    /// (`path_surface_vel`, the same seam as the grounded ride), or the hull flies out from under
    /// the absolute-space freeze and the cling silently drops (2026-07-05 playtest). Only
    /// meaningful while `wall_touch > 0`; stale otherwise. Appended at the struct's END
    /// (positional bincode).
    pub wall_ink: i8,
    /// Ink slot of the LEDGE this fighter hangs on (`-1` = a hardcoded stage lip, the fixed
    /// old behavior; `>= 0` = a drawn/baked stroke's lip). While `LedgeHold`/`LedgeClimb` on an
    /// ink lip, the hang re-pins to `paths[ledge_ink]`'s `ledge_node` world point each frame, so a
    /// translating/rotating stroke carries it (the moving-body trap: no absolute-space freeze). The
    /// lip vanishing (path dead / node decayed / segment no longer `Ledge`) drops the hang to Air.
    /// Only meaningful in a ledge state; stale otherwise. Appended at the struct's END (positional
    /// bincode: never reorder/insert mid-struct).
    pub ledge_ink: i8,
    /// Node index within `paths[ledge_ink]` that IS the grabbed lip tip -- the moving-body re-pin
    /// key (`ledge_ink` doc). Meaningful only while `ledge_ink >= 0`. Appended at the struct's END
    /// (positional bincode).
    pub ledge_node: u8,
}

impl Fighter {
    /// One fighter spawned airborne above the stage at `x`, facing `facing` (+1/-1).
    pub fn spawn(x: f32, facing: f32) -> Self {
        Self {
            frame: 0,
            pos: Vector2::new(x, 250.0),
            vel: Vector2::ZERO,
            state: CharState::Air,
            facing,
            air_jumps: 1,
            air_dodges: 1,
            fast_falling: false,
            full_hop: true,
            buf: [Slot::default(); N_LANE],
            autohop_aerial: false,
            intangible: false,
            regrab_lock: 0,
            ground_plat: -1,
            ground_ink: -1,
            hit_cd: [[0; MAX_PLAYERS]; MAX_HB],
            hitlag: 0,
            damage: 0.0,
            hitstun: 0,
            holding: -1,
            coyote: 0,
            invuln: 0,
            grab_link: -1,
            grab_timer: 0,
            tech_buf: 0,
            tumble: false,
            wall_hit: 0,
            drop_buf: 0,
            flick_x_age: u8::MAX,
            flick_y_age: u8::MAX,
            stick_was_hard_x: false,
            stick_was_hard_y: false,
            cstick_held: false,
            b_reversed: false,
            pickup_hold: false,
            shield_hp: SHIELD_MAX,
            shield_stun: 0,
            charge: 0,
            wall_touch: 0,
            wall_nx: 0.0,
            badges: 0,
            char_id: 0,
            arm: 0,
            arm_cd: 0,
            qb_cd: 0,
            catch_win: 0,
            ac_gas: 0,
            badge_gas: [0; N_BADGE_BITS],
            cling_used: 0,
            dive_latch: false,
            station: -1,
            wall_ink: -1,
            ledge_ink: -1,
            ledge_node: 0,
        }
    }

    /// Is a badge's passive mod active on this fighter?
    #[inline]
    pub fn has_badge(&self, b: Badge) -> bool {
        self.badges & b as u8 != 0
    }

    /// The action currently buffered in a lane, or `None` if the lane is empty/expired.
    #[inline]
    pub(crate) fn live(&self, l: Lane) -> Action {
        let s = &self.buf[l as usize];
        if s.timer > 0 { s.action } else { Action::None }
    }

    /// Record an edge into its lane: timer = the action's window + 1 (so the press frame always
    /// counts, since aging has already run this frame). Newer presses overwrite the lane.
    #[inline]
    pub(crate) fn record(&mut self, l: Lane, a: Action, aim: Vector2, t: &Tune) {
        self.buf[l as usize] = Slot {
            action: a,
            timer: a.window(t) + 1,
            aim,
        };
    }

    #[inline]
    pub(crate) fn clear_lane(&mut self, l: Lane) {
        self.buf[l as usize] = Slot::default();
    }

    /// Re-arm every hitbox of a fresh swing: zero the per-box, per-victim re-hit grid so the new
    /// move's boxes can all connect. Called on entering any attack state (replaces `attack_hit=false`).
    #[inline]
    pub(crate) fn arm_hits(&mut self) {
        self.hit_cd = [[0; MAX_PLAYERS]; MAX_HB];
    }

    /// Tick the re-hit grid down one frame (called once per active frame in `reduce_next_state`).
    #[inline]
    pub(crate) fn tick_hit_cd(&mut self) {
        for row in &mut self.hit_cd {
            for c in row {
                if *c > 0 {
                    *c -= 1;
                }
            }
        }
    }

    /// True if standing on ANY surface (a stage platform OR drawn ink) -- the single grounded
    /// read every FSM/physics site should route through instead of testing `ground_plat` (or
    /// `ground_plat < 0` for airborne) directly. `ground_plat == 0` doubles as the ink-landing
    /// sentinel (`set_ground`'s Ink arm, body/mod.rs), which this collapses along with the plain
    /// "on PLATFORMS[0]" case -- both mean "on a floor". Stage-is-ink keystone slice A: names the
    /// concept so a later slice can delete `ground_plat` behind this one signature.
    #[inline]
    pub fn grounded(&self) -> bool {
        self.ground_plat >= 0
    }

    /// True if the surface under `grounded()` is specifically drawn ink (a `SimState.paths`
    /// slot), not stage `PLATFORMS`. Independent of `grounded()`'s own ambiguity -- this is the
    /// disambiguator `set_ground`'s two writable fields exist to carry.
    #[inline]
    pub fn on_ink(&self) -> bool {
        self.ground_ink >= 0
    }

    /// Lossless reconstruction of `set_ground`'s owner from the two raw fields: `None` while
    /// airborne, `Ink` when riding a drawn stroke, `Platform` otherwise (covers the `ground_plat
    /// == 0` ink-landing sentinel by checking `ground_ink` FIRST). Read-only mirror of
    /// `set_ground` (body/mod.rs) -- keep the two in sync if that write seam ever changes shape.
    #[inline]
    pub fn ground_owner(&self) -> Option<SurfOwner> {
        if self.ground_plat < 0 {
            None
        } else if self.ground_ink >= 0 {
            Some(SurfOwner::Ink(self.ground_ink as u8))
        } else {
            Some(SurfOwner::Platform(self.ground_plat as u8))
        }
    }

    /// Debug/inspection accessors (the panel reads these; nothing else needs the lane indices).
    pub fn move_buffer(&self) -> Slot {
        self.buf[Lane::Movement as usize]
    }
    pub fn aerial_buffer_frames(&self) -> i64 {
        self.buf[Lane::Aerial as usize].timer
    }
    pub fn attack_buffer_frames(&self) -> i64 {
        self.buf[Lane::Attack as usize].timer
    }

    pub fn state_name(&self) -> &'static str {
        match self.state {
            CharState::Stand => "STAND",
            CharState::Walk => "WALK",
            CharState::Dash => "DASH",
            CharState::Run => "RUN",
            CharState::Turn => "TURN",
            CharState::Skid => "SKID",
            CharState::Crouch => "CROUCH",
            CharState::JumpSquat => "JUMPSQUAT",
            CharState::Air => "AIR",
            CharState::Landing => "LANDING",
            CharState::Shield => "SHIELD",
            CharState::SpotDodge => "SPOTDODGE",
            CharState::Roll => "ROLL",
            CharState::AirDodge => "AIRDODGE",
            CharState::LedgeHold => "LEDGE_HOLD",
            CharState::LedgeClimb => "LEDGE_CLIMB",
            CharState::Jab => "JAB",
            CharState::Nair => "NAIR",
            CharState::Fair => "FAIR",
            CharState::Bair => "BAIR",
            CharState::Uair => "UAIR",
            CharState::Dair => "DAIR",
            CharState::Dtilt => "DTILT",
            CharState::Ftilt => "FTILT",
            CharState::Utilt => "UTILT",
            CharState::Fsmash => "FSMASH",
            CharState::Usmash => "USMASH",
            CharState::Dsmash => "DSMASH",
            CharState::DashAttack => "DASHATK",
            CharState::Grab => "GRAB",
            CharState::GrabHold => "GRAB_HOLD",
            CharState::Grabbed => "GRABBED",
            CharState::Knockdown => "KNOCKDOWN",
            CharState::Getup => "GETUP",
            CharState::TechInPlace => "TECH",
            CharState::TechWall => "TECH_WALL",
            CharState::TechRoll => "TECH_ROLL",
            CharState::SpecialN => "SPECIAL_N",
            CharState::SpecialS => "SPECIAL_S",
            CharState::SpecialU => "SPECIAL_U",
            CharState::SpecialD => "SPECIAL_D",
            CharState::SpecialLandN => "SPECIAL_LAND_N",
            CharState::SpecialLandS => "SPECIAL_LAND_S",
            CharState::SpecialLandU => "SPECIAL_LAND_U",
            CharState::SpecialLandD => "SPECIAL_LAND_D",
            CharState::Helpless => "HELPLESS",
            CharState::Launched => "LAUNCHED",
            CharState::Crawl => "CRAWL",
            CharState::LedgeAttack => "LEDGE_ATK",
            CharState::LedgeRoll => "LEDGE_ROLL",
            CharState::GetupAttack => "GETUP_ATK",
            CharState::Rebound => "REBOUND",
            CharState::ShieldBreak => "SHIELD_BREAK",
        }
    }
}

/// Which side to respawn on after a blast-zone KO (keep the fighter on its half of the stage).
fn spawn_x(x: f32) -> f32 {
    if x < 600.0 { 480.0 } else { 720.0 }
}

/// Fresh fighter after a KO: clean spawn (0%) plus a window of i-frames so the player isn't combo'd
/// off the respawn point. `t.spawn_iframes` sizes the window. Badges are stock-scoped wear items,
/// not permanent mods: a KO strips every badge bit, and `Fighter::spawn` already zeroes `badges`
/// (plus `ac_gas`/`arm`/`arm_cd`/`qb_cd`, meaningless without a badge) — so there's nothing to carry
/// over here. Re-pick-up re-arms. The character tag is the one thing that DOES carry over.
pub(crate) fn respawn(old: &Fighter, t: &Tune) -> Fighter {
    let mut f = Fighter::spawn(spawn_x(old.pos.x), old.facing);
    f.invuln = t.spawn_iframes as u8;
    f.char_id = old.char_id;
    f
}
