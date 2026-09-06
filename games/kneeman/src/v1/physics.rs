//! Kinematics: the motion morphisms the FSM and integrator apply once a `CharState` is chosen.
//! Trajectory DI, air drift, air dodge, ledge snap, plus the unit/threshold constants and the
//! scalar math helpers. Pure value-in/value-out; re-exported at the crate root.

use crate::v1::arena::InkNode;
use crate::v1::body::Lip;
use crate::v1::{
    CharState, DT, FLOOR_LEFT, FLOOR_RIGHT, FPS, Fighter, GROUND_Y, InkPath, InputFrame,
    LEDGE_HANG_DY, Lane, MAX_DRAWN, PLATFORMS, PX_PER_UNIT, Tune, Vector2,
};

/// One frame of a GROUNDED special's platform pin (the planted punch): slide x by the move's
/// own velocity, hold y on the platform top -- but a `move_x` that carries the fighter past
/// the lip FALLS OUT as an aerial special (coyote grace armed) instead of pinning a "grounded"
/// fighter in space past the span (found by the net replay fixture: a grounded neutral-B
/// sliding off the right platform's edge tripped `on_real_floor`'s drift assert).
pub(crate) fn special_plat_pin(n: &mut Fighter, t: &Tune) {
    let p = PLATFORMS[n.ground_plat.clamp(0, PLATFORMS.len() as i32 - 1) as usize];
    n.pos.x += n.vel.x * DT;
    if n.pos.x < p.left || n.pos.x > p.right {
        n.ground_plat = -1;
        n.coyote = t.coyote_frames as u8;
    } else {
        n.pos.y = p.y;
        n.vel.y = 0.0;
    }
}

pub(crate) const DASH_THRESH: f32 = 0.5; // |stick| past this from neutral = dash (keyboard digital is always 1.0)
pub(crate) const WALK_THRESH: f32 = 0.25; // |stick| past this but under DASH = walk (needs analog stick)
pub(crate) const STOP_EPS: f32 = 1.0; // |vel.x| under this in a braking state snaps to 0
pub(crate) const WALL_TILT_FRAMES: i64 = 12; // how long after a wall bounce the shell tilts the sprite (cosmetic)
pub(crate) const DUMMY_FRICTION: f32 = 1200.0; // px/s^2 the dummy's knockback slide bleeds
pub(crate) const HITLAG_PER_DMG: f32 = 0.8; // impact-freeze frames per point of damage
// units/frame      -> px/s    (a velocity)
pub(crate) fn vel(u: f32) -> f32 {
    u * FPS * PX_PER_UNIT
}
// units/frame^2    -> px/s^2  (an acceleration)
pub(crate) fn acc(u: f32) -> f32 {
    u * FPS * FPS * PX_PER_UNIT
}

pub(crate) fn sign(x: f32) -> f32 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// Trajectory DI: the victim's stick rotates a launch toward its component perpendicular to the
/// knockback, up to `max_deg`. Speed is untouched -- only the angle -- so you can steer a launch
/// toward the stage to live, but never cancel your own knockback. Pure, so it rolls back cleanly.
pub(crate) fn apply_di(vel: Vector2, stick: Vector2, max_deg: f32) -> Vector2 {
    let speed = vel.length();
    if speed < 1.0 || stick.length() < 0.3 {
        return vel; // no knockback worth steering, or stick inside the deadzone
    }
    let u = vel / speed; // unit trajectory
    let s = stick.clamp_length_max(1.0);
    let cross = (u.x * s.y - u.y * s.x).clamp(-1.0, 1.0); // signed perpendicular component
    let (sin, cos) = (max_deg.to_radians() * cross).sin_cos();
    Vector2::new(vel.x * cos - vel.y * sin, vel.x * sin + vel.y * cos)
}

/// The aim to use for a buffered air dodge: the movement lane's captured diagonal if set, else the
/// live stick.
pub(crate) fn dodge_aim(n: &Fighter, i: &InputFrame) -> Vector2 {
    let m = &n.buf[Lane::Movement as usize];
    if m.aim.length() > 0.3 {
        m.aim
    } else {
        Vector2::new(i.dir, i.aim_y)
    }
}

/// Directional air-dodge burst from a 2D aim (digital diagonals included). Neutral aim = a
/// dodge in place. Into the ground a frame later, the surviving horizontal becomes a wavedash.
pub(crate) fn do_airdodge(n: &mut Fighter, aim: Vector2, t: &Tune) {
    if n.air_dodges > 0 {
        n.air_dodges -= 1;
    }
    n.vel = if aim.length() > 0.01 {
        aim.normalize_or_zero() * t.airdodge_speed
    } else {
        Vector2::ZERO
    };
    n.fast_falling = false;
    n.state = CharState::AirDodge;
}

/// Where the catching hand reaches THIS frame: a constant up-forward offset from the feet
/// (`pos`). `toward` is the reach direction (+1 right / -1 left -- the lip's inward face).
/// UNUSED by the catch test since plans/ledge-ship-fixes.md #1 (the directional box reads feet
/// position directly, no hand offset): kept as the seam a future sampled hand bone slots into
/// for the VISUAL hand placement, so the signature survives even with no live caller today.
#[allow(dead_code)]
pub(crate) fn hand_anchor(n: &Fighter, toward: f32, t: &Tune) -> Vector2 {
    Vector2::new(n.pos.x + toward * t.hand_reach_x, n.pos.y - t.hand_rise)
}

/// Snap onto a lip: hang at a fixed drop below the tip, face inward, refresh air resources, and
/// remember WHICH lip (ink slot + node) so the hang can re-pin to a moving stroke each frame. A
/// stage lip (`owner < 0`) records `ledge_ink = -1`, restoring the fixed old behavior.
pub(crate) fn grab_ledge(n: &mut Fighter, t: &Tune, lip: &Lip) {
    n.pos = lip.point + Vector2::new(0.0, LEDGE_HANG_DY);
    n.vel = Vector2::ZERO;
    n.facing = lip.face;
    n.fast_falling = false;
    n.ledge_ink = lip.owner;
    n.ledge_node = lip.node;
    crate::v1::body::touch_refresh(n, t); // a ledge is a surface touch like any other
    n.state = CharState::LedgeHold;
}

/// Deliberate ledge-drop opt-out (plans/ledge-ship-fixes.md #4): fast-fall must be AVAILABLE
/// (`fast_falling` already true) AND freshly RE-INITIATED after that (a fresh `down_pressed` edge,
/// not held through the apex). Holding down through the apex sets `fast_falling` but produces no
/// edge afterward, so ledges still grab; a release + repress while already fast-falling IS the
/// edge, and the fast-fall speed itself carries the body past the catch band before the next
/// (edge-less) frame gets another chance. No new state: reads two fields that already exist.
pub(crate) fn ledge_drop_opt_out(n: &Fighter, i: &InputFrame) -> bool {
    n.fast_falling && i.down_pressed
}

/// Ledge-snap eligibility + catch, shared by the plain-Air fall and the hitstun slide (directed
/// 2026-07-04: hitstun does NOT gate a ledge catch -- launched past the lip still grabs it). The
/// catch is a command grab the INK (or the stage) owns: the fighter's FEET tested against a
/// Melee-shaped zone hanging below+outboard of the lip (plans/ledge-ship-fixes.md #1 -- replaces
/// the old X-only gate + hand-anchor circle; a body ABOVE the lip is never in the zone, closing
/// the "grabs everything falling past from above" turbo-glue). Checked over EVERY lip (ink + the
/// two stage lips, stage first so they keep priority) -- container lips (the hull's hatch flanks)
/// flow through the SAME test as any other lip now (no `allow_container` special-case): a crew
/// member dropping IN approaches a hatch lip from ABOVE (rejected by the ceiling gate) while a
/// body rising from inside toward the hatch approaches from below/beside (admitted), so the
/// directional zone alone separates entry from exit -- no flag needed (plans/ledge-domain.md).
/// Gates: falling fast enough, past the regrab lock, not holding away, in the zone. Returns
/// whether it caught; the CALLER owns what catching means for its state (the hitstun slide
/// consumes stun/tumble -- the override layer's "mechanic disposes" rule).
pub(crate) fn try_ledge_snap(
    n: &mut Fighter,
    t: &Tune,
    paths: &[InkPath; MAX_DRAWN],
    nodes: &[InkNode],
    stick_x: f32,
) -> bool {
    if n.vel.y <= t.ledge_fall_eps || n.regrab_lock != 0 {
        return false;
    }
    for lip in crate::v1::body::LipSoup::collect(paths, nodes, t).lips() {
        // holding AWAY from the stage past the outward deadzone: bailing out, don't snap.
        if stick_x * lip.face < -0.5 {
            continue;
        }
        // The directional box, feet-relative. `lip.face` points INWARD (toward where the floor
        // continues), so `(lip.point - n.pos) * face` is POSITIVE when the feet sit OUTBOARD of
        // the lip (the sign flip from the old inboard-positive test at this same site) -- outboard
        // reads as a positive distance, matching the knob names below directly. +y is DOWN, so
        // "below the lip" is a larger `pos.y`.
        let dx = (lip.point.x - n.pos.x) * lip.face; // + outboard, - inboard
        let dy = n.pos.y - lip.point.y; // + below the lip, - above it
        if dx < -t.ledge_lip_bite || dx > t.ledge_reach_x {
            continue; // too far inboard (past the turn-back tolerance), or beyond outboard reach
        }
        if dy < -t.ledge_ceil || dy > t.ledge_reach_down {
            continue; // above the lip past the ceiling sliver, or below the hang depth
        }
        grab_ledge(n, t, lip);
        return true;
    }
    false
}

/// One frame of an ink hang's ride (integrate's ledge branch): re-pin the fighter to the grabbed
/// lip's CURRENT world point so a translating/rotating stroke carries the hang (the moving-body
/// trap -- never an absolute-space freeze). The lip vanishing (path dead / node decayed / segment
/// no longer `Ledge`) drops the hang to Air with coyote grace + a short regrab lock. A stage lip
/// (`ledge_ink < 0`) holds the fixed grab position (nothing to re-pin).
pub(crate) fn ledge_ride(
    n: &mut Fighter,
    paths: &[InkPath; MAX_DRAWN],
    nodes: &[InkNode],
    t: &Tune,
) {
    if n.ledge_ink < 0 {
        return;
    }
    match crate::v1::body::lip_world(paths, nodes, n.ledge_ink, n.ledge_node) {
        Some(point) => n.pos = point + Vector2::new(0.0, LEDGE_HANG_DY),
        None => {
            n.state = CharState::Air;
            n.ledge_ink = -1;
            n.coyote = t.coyote_frames as u8;
            n.regrab_lock = 12;
        }
    }
}

/// Climb up onto the grabbed ledge: an ink lip plants the feet on the lip node stepping inward
/// (`facing`) along its Floor segment, now grounded on that stroke; a stage lip (or an ink lip that
/// vanished mid-climb) takes the original hardcoded step onto the main floor. Clears the ledge ref.
pub(crate) fn ledge_climb(n: &mut Fighter, paths: &[InkPath; MAX_DRAWN], nodes: &[InkNode]) {
    if let Some(point) = crate::v1::body::lip_world(paths, nodes, n.ledge_ink, n.ledge_node) {
        n.pos = Vector2::new(point.x + n.facing * 30.0, point.y);
        n.vel = Vector2::ZERO;
        n.ground_plat = 0; // the grounded-on-ink convention (set_ground's ink case)
        n.ground_ink = n.ledge_ink;
        n.ledge_ink = -1;
        return;
    }
    n.pos.x = if n.facing > 0.0 {
        FLOOR_LEFT + 30.0
    } else {
        FLOOR_RIGHT - 30.0
    };
    n.pos.y = GROUND_Y;
    n.vel = Vector2::ZERO;
    n.ground_plat = 0;
    n.ground_ink = -1;
    n.ledge_ink = -1;
}

/// Horizontal air drift (Ultimate-style, full bidirectional control): hold a direction to
/// accelerate toward the drift cap at air_accel (crisp turn, full strength when reversing);
/// momentum ABOVE the cap in the held direction is preserved (light drag only), so dash-jumps
/// keep their speed.
pub(crate) fn air_drift(n: &mut Fighter, i: &InputFrame, t: &Tune, sgn: f32) {
    let target = i.dir * t.air_speed;
    if sgn == 0.0 {
        n.vel.x = move_toward(n.vel.x, 0.0, t.air_friction * DT); // coast
    } else if n.vel.x.abs() <= t.air_speed || sign(n.vel.x) != sgn {
        n.vel.x = move_toward(n.vel.x, target, t.air_accel * DT); // turn / accel
    } else {
        n.vel.x = move_toward(n.vel.x, target, t.air_friction * DT); // keep momentum
    }
}

pub(crate) fn move_toward(from: f32, to: f32, delta: f32) -> f32 {
    if (to - from).abs() <= delta {
        to
    } else {
        from + (to - from).signum() * delta
    }
}
