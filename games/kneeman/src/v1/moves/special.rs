//! Special moves (the B-slots): the `SpecialKind`/`SpecialMove` loadout records, the stick->slot
//! routing, and the per-frame run logic. Re-exported through `moves`.

use crate::v1::{
    Action, AttackData, CharState, DT, ECB_HALF_H, Fighter, Hitbox, InputFrame, Lane, Tune,
    Vector2, airborne, move_toward, sign,
};
use serde::{Deserialize, Serialize};

/// Special states map to a slot in `Tune.specials` (the seed for swappable move loadouts).
pub(crate) fn special_slot(st: CharState) -> Option<usize> {
    match st {
        CharState::SpecialN => Some(0),
        CharState::SpecialS => Some(1),
        CharState::SpecialU => Some(2),
        CharState::SpecialD => Some(3),
        _ => None,
    }
}
pub(crate) fn is_special(st: CharState) -> bool {
    special_slot(st).is_some() || special_landing_slot(st).is_some()
}

pub(crate) fn special_landing_slot(st: CharState) -> Option<usize> {
    match st {
        CharState::SpecialLandN => Some(0),
        CharState::SpecialLandS => Some(1),
        CharState::SpecialLandU => Some(2),
        CharState::SpecialLandD => Some(3),
        _ => None,
    }
}

pub(crate) fn run_special_landing(n: &mut Fighter, t: &Tune) {
    let slot = special_landing_slot(n.state).unwrap();
    let total = t.specials[slot].landing.map(|attack| attack.total()).unwrap_or(0);
    n.vel.x = move_toward(n.vel.x, 0.0,
        if n.grounded() { t.ground_friction * DT } else { t.air_friction * DT });
    if !n.grounded() { n.vel.y = (n.vel.y + t.gravity * DT).min(t.max_fall); }
    if n.frame >= total - 1 {
        n.state = if n.grounded() { CharState::Stand } else { CharState::Air };
    }
}

/// Which special the stick selects at the press: up / down / side / neutral.
pub(crate) fn special_dir(aim: Vector2) -> usize {
    if aim.y <= -0.5 {
        2 // up
    } else if aim.y >= 0.5 {
        3 // down
    } else if aim.x.abs() >= 0.5 {
        1 // side
    } else {
        0 // neutral
    }
}

/// The (live) attack definition for a state, if it is one.
/// How a special moves the fighter while it runs. The hitbox/frames/knockback live in `hit`
/// (reuses the whole attack pipeline: `attack_for` -> `active_hitbox` -> `resolve_combat`).
/// This is the seed for swappable move loadouts: a character's 4 B-slots are just `SpecialMove`s.
#[derive(Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum SpecialKind {
    Punch, // planted heavy hit (neutral-B): brakes to a stop, no travel
    Lunge, // forward burst (side-B)
    Rise,  // upward recovery burst (up-B); ends in Helpless if still airborne
    Fall,  // downward drive (down-B)
    // Falcon up-B command grab (queue-2026-07-03 item 4): a STATIONARY hug that captures instead of
    // hitting, then explodes with a fixed-angle launch. Wind-up hang, then a rising travel with the
    // hug box live through the climb. Runs inside `SpecialU` but emits no combat hitbox; `run_special`
    // routes it out of the normal launch pipeline on entry, and `resolve_grab`
    // owns the catch/latch/explosion. Appended at the enum's END: bincode is positional (never reorder).
    DiveGrab,
    // Appended for positional bincode compatibility. Restores jumps only on airborne completion.
    FallRefreshJump,
    // Recovery is the ending phase. Keep FallRefreshJump's published completion semantics.
    FallRefreshOnRecovery,
    // Authored ground/air launch, velocity-preserving travel, then recovery-entry jump refresh.
    Kick { ground_speed: f32 },
}

#[derive(Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpecialMove {
    pub kind: SpecialKind,
    pub hit: AttackData, // frames + hitbox + knockback
    pub move_x: f32,     // forward-relative horizontal burst at the active window (px/s)
    pub move_y: f32,     // vertical burst (negative = up) at the active window (px/s)
    // While airborne and running this move, gravity does not pull: the fighter holds its burst
    // velocity (Ness/Lucas-style floaty up-B). Off => normal gravity + air drift, so the move arcs
    // and the fighter falls. No move "air-stalls" implicitly anymore; it's opt-in per loadout slot.
    pub no_gravity: bool,
    // Falcon Punch "hang time": while airborne and frame < (startup + active), |vel.y| is clamped
    // to this value. Bidirectional damper -- a fighter rising slows their rise, a fighter falling
    // slows their fall. 0 = off (normal gravity). Active frames + the wind-up together: the slow
    // window starts at the move's first frame and ends when active closes; recovery returns to
    // full gravity so the fighter actually drops out of the move. The launch burst at `hit.startup`
    // still fires (it sets vel.y to `move_y`), but the clamp catches it on subsequent frames.
    pub hang_vel: f32,
    /// Optional contact attack selected by SpecialRecovery. Its clock starts at ground contact.
    #[serde(default)]
    pub landing: Option<AttackData>,
}

impl SpecialMove {
    // Authored Game3 speeds, not decoded PM attributes. Keep existing hit/recovery timing.
    pub(crate) const FALCON_KICK: Self = Self {
        kind: SpecialKind::Kick { ground_speed: 900.0 },
        move_x: 900.0,
        move_y: 900.0,
        hit: AttackData { land_cancel: crate::v1::LandCancel::SpecialRecovery, ..Self::DROP.hit },
        // Reference combat values; one active tick and 6 px/unit geometry remain authored.
        landing: Some({
            let hit = Hitbox {
                start: 0, len: 1, r: 30.0, damage: 10.0, angle: 80.0, bkb: 65.0, kbg: 35.0,
                targets: crate::v1::HitTargets::Ground, ..Hitbox::NONE
            };
            let mut attack = AttackData::new(0, 18, [
                Hitbox { off: Vector2::new(51.0, -24.0), ..hit },
                Hitbox { off: Vector2::new(-51.0, -24.0), ..hit },
                Hitbox { off: Vector2::new(0.0, -24.0), ..hit },
                Hitbox::NONE,
            ], 3);
            attack.land_cancel = crate::v1::LandCancel::Continue;
            attack
        }),
        ..Self::DROP
    };
    // Default kit (Falcon-ish): heavy neutral-B punch, a side lunge, a rising recovery, a down drive.
    pub(crate) const PUNCH: Self = Self {
        kind: SpecialKind::Punch,
        hit: AttackData::one(
            14,
            4,
            26,
            Hitbox {
                off: Vector2::new(58.0, -60.0),
                r: 46.0,
                damage: 22.0,
                angle: 38.0,
                bkb: 60.0,
                kbg: 168.0,
                ..Hitbox::NONE
            },
        ),
        // Falcon-punch surge: lunge forward into the hit, not a mid-air hover. Grounded, friction
        // bleeds it to the planted step; aerial, gravity arcs him down after the lunge.
        move_x: 380.0,
        move_y: -60.0,
        no_gravity: false,
        // Hang time through windup+active: rising or falling, the body slows so the punch winds up
        // visibly in the air (Melee Falcon Punch vertical damp). 60 px/s is a slow hover -- rising
        // fighters stall, falling fighters hang. Clamp lifts the moment active closes (frame 18),
        // so recovery returns to full gravity and he actually drops out of the whiff.
        hang_vel: 60.0,
        landing: None,
    };
    pub(crate) const LUNGE: Self = Self {
        kind: SpecialKind::Lunge,
        hit: AttackData::one(
            8,
            6,
            22,
            Hitbox {
                off: Vector2::new(60.0, -58.0),
                r: 42.0,
                damage: 9.0,
                angle: 55.0,
                bkb: 40.0,
                kbg: 104.0,
                ..Hitbox::NONE
            },
        ),
        move_x: 900.0,
        move_y: -120.0,
        no_gravity: false,
        hang_vel: 0.0,
        landing: None,
    };
    pub(crate) const RISE: Self = Self {
        kind: SpecialKind::Rise,
        hit: AttackData::one(
            6,
            8,
            22,
            Hitbox {
                off: Vector2::new(20.0, -90.0),
                r: 44.0,
                damage: 7.0,
                angle: 80.0,
                bkb: 48.0,
                kbg: 76.0,
                ..Hitbox::NONE
            },
        ),
        move_x: 380.0,
        move_y: -1500.0,
        no_gravity: false,
        hang_vel: 0.0,
        landing: None,
    };
    pub(crate) const DROP: Self = Self {
        kind: SpecialKind::Fall,
        hit: AttackData::one(
            8,
            10,
            18,
            Hitbox {
                off: Vector2::new(24.0, 10.0),
                r: 44.0,
                damage: 10.0,
                angle: -68.0,
                bkb: 44.0,
                kbg: 84.0,
                ..Hitbox::NONE // downward: a spike
            },
        ),
        move_x: 220.0,
        move_y: 700.0,
        no_gravity: false,
        hang_vel: 0.0,
        landing: None,
    };
    // Falcon up-B: the command grab. A wind-up hang (startup, braked in space like the punch),
    // then the rise fires at `b.start` with the hug live through the climb -- so both-grounded
    // connects on the early frames and the box travels up with him. `hit` is deliberately
    // BOXLESS (`nbox` 0): the move emits
    // no combat hitbox, so `resolve_combat` / `attack_for(SpecialU).live_boxes()` see nothing and it
    // can only CATCH, never hit. `resolve_grab` reads `hit.boxes[0]` directly as the connect box +
    // explosion payload: `start/len` are the hug window on the shared `f.frame` clock, `r` the hug
    // radius (a body-centered circle), and `angle/damage/bkb/kbg` the fixed-angle launch on the
    // boom. 45deg up-and-away, mirrored by the attacker's facing, is the "actual Captain Falcon" line.
    pub(crate) const FALCON_DIVE: Self = Self {
        kind: SpecialKind::DiveGrab,
        hit: AttackData::new(
            10, // startup: the wind-up HANG -- braked in space like the punch's plant, no travel yet
            20, // recovery: whiff endlag tail after the hug window closes
            [
                Hitbox {
                    id: 0,
                    start: 10, // hug window opens the same frame the rise launches
                    len: 26, // hug active through the rise: resolve_grab catches while f.frame is in [10, 36)
                    off: Vector2::new(0.0, -ECB_HALF_H), // body-centered: a hug, not a forward reach
                    r: 92.0,      // hug radius: the connect circle around the fighter
                    damage: 18.0, // the dive explosion's %
                    angle: 45.0,  // FIXED launch angle: up-and-away, mirrored by facing
                    bkb: 60.0,
                    kbg: 92.0,
                    ..Hitbox::NONE
                },
                Hitbox::NONE,
                Hitbox::NONE,
                Hitbox::NONE,
            ],
            0, // nbox 0: NO combat hitbox. The box above is data for resolve_grab, hidden from combat.
        ),
        move_x: 320.0, // stick-aimed drift once the rise launches (a touch under RISE's 380)
        move_y: -1300.0, // the rise burst at b.start; gravity arcs it back down over the window
        no_gravity: false, // gravity runs from launch (the hang phase zeroes vel explicitly instead)
        hang_vel: 0.0,
        landing: None,
    };
}

/// Consume a buffered special if one is live: pick the slot from the captured stick, enter the
/// matching state, face the stick on a side-B. Available from every actionable ground/air state.
pub(crate) fn try_special(n: &mut Fighter) -> bool {
    if n.live(Lane::Special) != Action::Special {
        return false;
    }
    let aim = n.buf[Lane::Special as usize].aim;
    n.clear_lane(Lane::Special);
    // ground_plat lingers at its grounded platform index after a normal jump (it's only cleared on
    // walk-off / drop-through), so a special entered from the air would read as grounded: the punch
    // plants and the integrator pins pos.y to the platform (the "B-air teleports me to ground" bug).
    // Re-derive it from whether we're actually airborne at the press.
    if airborne(n.state) {
        n.ground_plat = -1;
    }
    let slot = special_dir(aim);
    if slot == 1 && aim.x != 0.0 {
        n.facing = sign(aim.x); // side-B turns you toward the stick
    }
    n.state = match slot {
        0 => CharState::SpecialN,
        1 => CharState::SpecialS,
        2 => CharState::SpecialU,
        _ => CharState::SpecialD,
    };
    n.b_reversed = false; // a fresh special gets one B-reverse (see run_special)
    n.arm_hits();
    true
}

/// Run one frame of a special. The launch burst lands when the active window opens; gravity/friction
/// run by whether we're airborne (`ground_plat < 0`). Up-B ends in Helpless if it finishes in the air.
pub(crate) fn run_special(n: &mut Fighter, slot: usize, i: &InputFrame, t: &Tune) {
    let m = t.specials[slot];
    // Falcon up-B command grab (queue-2026-07-03 item 4, travel added 2026-07-04): a punch-style
    // wind-up hang, then the rise launches the same frame the hug window opens.
    // It runs inside `SpecialU` (no new CharState / sprite clip): the move emits NO combat hitbox
    // (FALCON_DIVE.hit is boxless -- `nbox` 0 -- so `resolve_combat` never fires), and its hug
    // box + fixed-angle explosion are cross-fighter, owned by `resolve_grab`. Hold the body still
    // through startup, leave the ground at launch, and on a WHIFF (the hug window + endlag elapsed
    // with no catch) drop to Helpless -- the same special-fall a real Falcon Dive that grabbed
    // nothing lands in. Char-gated purely by the loadout slot: only a `DiveGrab` up-B reaches this.
    // A connect flips this fighter to `GrabHold` (with `dive_latch`) before it can whiff out.
    if m.kind == SpecialKind::DiveGrab {
        let b = m.hit.boxes[0];
        if n.frame < b.start {
            // wind-up hang: braked in space, the punch-style plant before the dive
            n.vel = Vector2::ZERO;
        } else {
            if n.frame == b.start {
                // Leaving support during the zero-velocity wind-up would immediately land-cancel.
                n.ground_plat = -1;
                // lift-off the same frame the hug window opens: the connect circle rides
                // the rise (resolve_grab recomputes it from g.pos each frame), so a body-ground
                // grabber hugs a grounded foe on the first active frames before climbing away
                n.vel = Vector2::new(i.dir * m.move_x, m.move_y);
                n.fast_falling = false;
            }
            n.vel.y += t.gravity * DT;
            if n.vel.y > t.max_fall {
                n.vel.y = t.max_fall;
            }
        }
        if n.frame >= b.start + b.len + m.hit.recovery - 1 {
            n.state = CharState::Helpless;
        }
        return;
    }
    // B-reverse / wavebounce: one back-flick inside the early window flips facing AND mirrors the
    // horizontal momentum. Drifting forward off a jump, tapping B, then flicking back is the
    // wavebounce — the body kicks back the way it came while the move fires the other way. Once
    // per special; the up-B keeps its own stick-aimed launch (reversing a recovery is a trap).
    if !n.b_reversed
        && n.frame < t.b_rev_window
        && m.kind != SpecialKind::Rise
        && i.dir.abs() >= 0.5
        && sign(i.dir) == -n.facing
    {
        n.facing = -n.facing;
        n.vel.x = -n.vel.x;
        n.b_reversed = true;
    }
    if let SpecialKind::Kick { ground_speed } = m.kind {
        if n.frame >= m.hit.startup && n.frame < m.hit.active_end() {
            if n.frame == m.hit.startup {
                n.vel = if n.grounded() {
                    Vector2::new(n.facing * ground_speed, 0.0)
                } else {
                    Vector2::new(n.facing * m.move_x, m.move_y)
                };
                n.fast_falling = false;
            }
            // Velocity records the launch choice. Leaving a ledge keeps the horizontal drive;
            // contact resolution may change velocity, and hitlag already freezes the frame clock.
            return;
        }
    }
    // Up-B lifts off on frame 0 (instant recovery, no ground-snap); the rest burst at the active
    // window. The hitbox window (startup..) is independent of this movement timing.
    let launch_frame = if m.kind == SpecialKind::Rise {
        0
    } else {
        m.hit.startup
    };
    if n.frame == launch_frame {
        // Every kind fires its impulse vector (facing-relative, or stick-relative for the recovery).
        // Punch/Lunge/Fall drive along facing; Rise aims with the stick. Lunge/Rise leave the ground.
        match m.kind {
            SpecialKind::Punch => n.vel = Vector2::new(n.facing * m.move_x, m.move_y),
            SpecialKind::Lunge => {
                n.vel = Vector2::new(n.facing * m.move_x, m.move_y);
                n.ground_plat = -1;
            }
            SpecialKind::Rise => {
                n.vel = Vector2::new(i.dir * m.move_x, m.move_y);
                n.fast_falling = false;
                n.ground_plat = -1;
            }
            SpecialKind::Fall | SpecialKind::FallRefreshJump | SpecialKind::FallRefreshOnRecovery => {
                n.vel = Vector2::new(n.facing * m.move_x, m.move_y);
            }
            // DiveGrab is routed out at the top of `run_special` (stationary command grab), so it
            // never reaches this launch dispatch; the arm exists only for match exhaustiveness.
            SpecialKind::DiveGrab | SpecialKind::Kick { .. } => {}
        }
    }
    if !n.grounded() {
        if m.no_gravity {
            // floaty move (e.g. a PSI recovery): hold the burst, only bleed horizontal slowly. No
            // gravity, so it hangs instead of arcing — the opt-in air-stall, never the default.
            n.vel.x = move_toward(n.vel.x, 0.0, t.air_friction * DT);
        } else {
            n.vel.x = move_toward(n.vel.x, i.dir * t.air_speed * 0.6, t.air_accel * DT);
            n.vel.y += t.gravity * DT;
            if n.vel.y > t.max_fall {
                n.vel.y = t.max_fall;
            }
        }
        // Falcon Punch "hang time" (m.hang_vel > 0): clamp |vel.y| during windup+active so the
        // fighter slows in BOTH directions -- rising stalls, falling hangs. Window is frame 0
        // .. startup+active_len so the lead-up AND the active frames dampen; recovery returns to
        // full gravity so the fighter actually drops out of the whiff. The launch burst at
        // `hit.startup` still sets vel.y (PUNCH: -60); the clamp catches it the next frame.
        // Skip for DiveGrab (it has its own physics branch above and returns early).
        if m.hang_vel > 0.0 && m.kind != SpecialKind::DiveGrab {
            // First active hitbox end frame: m.hit.boxes[0].start + len. For one-box moves this
            // is the active window; multi-box moves (none currently use hang_vel) would close at
            // the last box's end. Use the first box as the canonical active window.
            let active_end = m.hit.boxes[0].start + m.hit.boxes[0].len;
            if n.frame < active_end {
                let cap = m.hang_vel;
                if n.vel.y > cap {
                    n.vel.y = cap;
                } else if n.vel.y < -cap {
                    n.vel.y = -cap;
                }
            }
        }
    } else {
        // grounded: bleed horizontal to a planted stop
        n.vel.x = move_toward(n.vel.x, 0.0, t.ground_friction * DT);
    }
    // The existing recovery interval supplies the authored ending phase. Refresh once on entry,
    // while SpecialD remains locked; no extra mutable phase field is needed in snapshots.
    if matches!(m.kind, SpecialKind::FallRefreshOnRecovery | SpecialKind::Kick { .. })
        && n.frame == m.hit.active_end()
        && !n.grounded()
    {
        n.air_jumps = t.max_air_jumps.clamp(0, u8::MAX as i64) as u8;
    }
    if n.frame >= m.hit.total() - 1 {
        if m.kind == SpecialKind::FallRefreshJump && !n.grounded() {
            n.air_jumps = t.max_air_jumps.clamp(0, u8::MAX as i64) as u8;
        }
        n.state = if m.kind == SpecialKind::Rise && !n.grounded() {
            CharState::Helpless
        } else if !n.grounded() {
            CharState::Air
        } else {
            CharState::Stand
        };
    }
}
