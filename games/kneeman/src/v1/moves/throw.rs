//! Grabs + throws: the `ThrowData` launch records, the grab catch/hold/pummel/mash-out resolution,
//! and the throw release. Cross-fighter, so `resolve_grab` owns the held pair. Re-exported through `moves`.

use crate::v1::{
    Aim, CharState, ECB_HALF_H, Fighter, HITLAG_PER_DMG, InputFrame, SpecialKind, Swing, ThrowDir,
    Tune, Vector2, hurtbox, sign, strike,
};
use serde::{Deserialize, Serialize};

/// A throw's launch: the grab's payoff. No frame windows (the throw fires the frame it's chosen);
/// just damage + the knockback curve, indexed fwd/back/up/down in `Tune.throws`.
#[derive(Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThrowData {
    pub damage: f32,
    pub kb_base: f32,
    pub kb_scale: f32,
    pub kb_angle: f32, // degrees, 0 = forward, 90 = straight up
}

impl ThrowData {
    pub(crate) const FWD: Self = Self {
        damage: 8.0,
        kb_base: 520.0,
        kb_scale: 4.2,
        kb_angle: 48.0,
    };
    pub(crate) const BACK: Self = Self {
        damage: 10.0,
        kb_base: 620.0,
        kb_scale: 4.6,
        kb_angle: 50.0,
    };
    pub(crate) const UP: Self = Self {
        damage: 7.0,
        kb_base: 560.0,
        kb_scale: 4.4,
        kb_angle: 88.0,
    };
    pub(crate) const DOWN: Self = Self {
        damage: 6.0,
        kb_base: 440.0,
        kb_scale: 3.6,
        kb_angle: 72.0,
    };
}

// --- grabs ---------------------------------------------------------------------------------------

pub(crate) const GRAB_HELD_X: f32 = 64.0; // how far in front of the grabber the victim is pinned
pub(crate) const GRAB_CATCH_R: f32 = 36.0; // slop added to the reach-vs-hurtbox catch test
pub(crate) const KNOCKDOWN_LOCK: i64 = 6; // floored frames before any getup option is allowed

/// Falcon up-B command grab (queue-2026-07-03 item 4): frames the pair hangs frozen after a
/// command-grab catch before the fixed-angle explosion. ~half a second at 60Hz -- long enough to
/// read as a hug, short enough to be a real move. NOT escapable (no mash chips it).
pub(crate) const CMD_LATCH_FRAMES: i64 = 34;

/// Backward drift (px/s) of the attacker's post-explosion flyaway flip -- up at `airjump_v`,
/// away from the facing at this. Enough to visibly flip off the victim, small next to
/// `air_speed` (~600) so it never reads as a launch.
pub(crate) const CMD_FLYAWAY_X: f32 = 160.0;

// Direction-only throw select: the grabber's stick alone picks + fires the throw, no button.
pub(crate) const THROW_DEFLECT_THRESH: f32 = 0.4; // stick magnitude that counts as a throw pick
pub(crate) const GRAB_HOLD_GRACE: i64 = 3; // frames of a fresh hold where the stick is ignored --
// covers the run-in stick still deflected at the catch

/// Count fresh button edges this frame — the victim "mashes" these to shorten the hold.
fn mash_count(i: &InputFrame) -> i64 {
    (i.attack as i64)
        + (i.jump as i64)
        + (i.shorthop as i64)
        + (i.special as i64)
        + (i.grab as i64)
        + (i.shield_pressed as i64)
}

/// Throw direction from the grabber's stick at release: up / down / back / forward (default).
fn throw_dir(i: &InputFrame, facing: f32) -> usize {
    if i.aim_y <= -THROW_DEFLECT_THRESH {
        2 // up
    } else if i.aim_y >= THROW_DEFLECT_THRESH {
        3 // down
    } else if sign(i.dir) == -facing && i.dir.abs() >= THROW_DEFLECT_THRESH {
        1 // back
    } else {
        0 // forward
    }
}

/// Has the grabber's stick crossed the throw-pick threshold on either axis this frame?
fn stick_deflected(i: &InputFrame) -> bool {
    i.dir.abs() >= THROW_DEFLECT_THRESH || i.aim_y.abs() >= THROW_DEFLECT_THRESH
}

/// Cut both fighters loose from a hold and stand them up (victim mashed out, or the grabber let go).
fn release_grab(g: &mut Fighter, v: &mut Fighter) {
    g.state = CharState::Stand;
    g.frame = 0;
    g.grab_link = -1;
    g.grab_timer = 0;
    g.dive_latch = false;
    // The victim is let go AT THE SLAVE POS -- held off its feet, possibly past the edge of
    // whatever it was standing on when caught (its ground refs were cleared at the catch).
    // Air is the honest state: standing ground re-derives on the next landing sweep; a Stand
    // here could read "grounded" hanging in space (the drift assert the net replay fixture
    // tripped the day the falcon row armed).
    v.state = CharState::Air;
    v.frame = 0;
    v.grab_link = -1;
    v.grab_timer = 0;
    v.vel = Vector2::ZERO;
}

/// Launch the victim out of a throw, then unlink both. Grabber recovers to neutral.
fn do_throw(g: &mut Fighter, v: &mut Fighter, g_in: &InputFrame, t: &Tune) {
    let dir = throw_dir(g_in, g.facing);
    let td = t.throws[dir];
    v.damage += td.damage;
    let speed = (td.kb_base + td.kb_scale * v.damage) * t.knockback_mult;
    let sign_x = if dir == 1 { -g.facing } else { g.facing }; // back-throw fires behind the grabber
    let ang = td.kb_angle.to_radians();
    v.vel = Vector2::new(ang.cos() * sign_x, -ang.sin()) * speed;
    v.hitstun = (speed * 0.12) as i64;
    v.tumble = speed > t.tumble_speed;
    let freeze = (td.damage * HITLAG_PER_DMG) as i64 + 4;
    v.hitlag = freeze;
    g.hitlag = freeze;
    v.state = CharState::Air;
    v.ground_plat = -1;
    v.grab_link = -1;
    v.grab_timer = 0;
    g.state = CharState::Stand;
    g.frame = 0;
    g.grab_link = -1;
    g.grab_timer = 0;
}

/// Cross-fighter grab resolution for one ordered pair (grabber `g`, would-be victim `v`). Handles
/// the catch during the grab's active window, then maintains the hold: slaves the victim to the
/// grabber, runs pummel / throw on the grabber's inputs, and the victim's mash-out. Called both
/// orderings each frame (like `resolve_combat`); only the side actually grabbing does work.
///
/// Returns `Some(pos)` on the frame a Falcon command grab EXPLODES (the shell paints the cheesy
/// fireball there); `None` every other frame.
pub(crate) fn resolve_grab(
    g: &mut Fighter,
    v: &mut Fighter,
    gi: i8,
    vi: i8,
    g_in: &InputFrame,
    v_in: &InputFrame,
    t: &Tune,
) -> Option<Vector2> {
    // 0) Falcon up-B command grab CATCH: a hug box AROUND the body (not a forward reach) that rides
    // the rise (recomputed from `g.pos` each frame)
    // captures a catchable foe, airborne or grounded, while `SpecialU` runs a `DiveGrab` loadout.
    // This is the general air-grab unlock realized for Falcon: the connect works with both fighters
    // airborne, and the pair then hangs in place (both velocities frozen) for the latch. On catch we
    // reuse the shared `GrabHold`/`Grabbed` pair, flagged `dive_latch` so the maintenance arm below
    // runs the fixed-delay explosion instead of pummel/throw.
    if g.state == CharState::SpecialU
        && t.specials[2].kind == SpecialKind::DiveGrab
        && g.grab_link < 0
    {
        let b = crate::v1::attack_for(t, CharState::SpecialU, g.special_started_air).unwrap().boxes[0];
        let active = g.frame >= b.start && g.frame < b.start + b.len;
        let catchable = !matches!(v.state, CharState::Grabbed | CharState::GrabHold)
            && v.invuln == 0
            && !v.intangible
            && v.hitstun == 0;
        let center = g.pos + b.off; // body-centered hug circle (b.off.x is 0: facing-agnostic)
        let (vc, vr) = hurtbox(v);
        if active && catchable && (center - vc).length() <= b.r + vr {
            g.state = CharState::GrabHold;
            g.frame = 0;
            g.grab_link = vi;
            g.grab_timer = CMD_LATCH_FRAMES;
            g.vel = Vector2::ZERO;
            g.dive_latch = true;
            v.state = CharState::Grabbed;
            v.frame = 0;
            v.grab_link = gi;
            v.grab_timer = CMD_LATCH_FRAMES;
            v.vel = Vector2::ZERO;
            v.hitstun = 0;
            // held = off your feet: stale ground refs on a teleported victim read "grounded"
            // hanging in space and trip `on_real_floor`'s drift assert.
            v.ground_plat = -1;
            v.ground_ink = -1;
        }
        return None;
    }

    // 1) catch: the grab's reach overlaps a catchable victim during the active window.
    if g.state == CharState::Grab && g.grab_link < 0 {
        let active = g.frame >= t.grab_startup && g.frame < t.grab_startup + t.grab_active;
        let catchable = !matches!(v.state, CharState::Grabbed | CharState::GrabHold)
            && v.invuln == 0
            && !v.intangible
            && v.hitstun == 0;
        let reach = g.pos + Vector2::new(g.facing * t.grab_range, -ECB_HALF_H);
        let (vc, vr) = hurtbox(v);
        if active && catchable && (reach - vc).length() <= vr + GRAB_CATCH_R {
            g.state = CharState::GrabHold;
            g.frame = 0;
            g.grab_link = vi;
            g.grab_timer = t.grab_hold;
            g.vel = Vector2::ZERO;
            v.state = CharState::Grabbed;
            v.frame = 0;
            v.grab_link = gi;
            v.grab_timer = t.grab_hold;
            v.vel = Vector2::ZERO;
            v.hitstun = 0;
            v.ground_plat = -1; // held = off your feet (see the dive-catch comment above)
            v.ground_ink = -1;
        }
        return None;
    }

    // 2) maintain an existing hold (only the matching linked pair).
    if g.state == CharState::GrabHold && g.grab_link == vi && v.grab_link == gi {
        // Command-grab latch (Falcon up-B): both hang in place (the held early-return froze their
        // velocities), the victim can't mash out, and after a fixed delay the pair EXPLODES with a
        // fixed-angle launch. Runs instead of the pummel/throw hold below.
        if g.dive_latch {
            v.state = CharState::Grabbed;
            v.pos = g.pos; // hug: pin the victim onto the grabber (both frozen)
            // The pin TELEPORTS the victim to the grabber -- usually mid-air, always off its own
            // floor -- so stale ground refs would read "grounded" hanging in space and trip
            // `on_real_floor`'s drift assert (found by the net replay fixture the day row 1
            // armed the dive).
            v.ground_plat = -1;
            v.ground_ink = -1;
            v.vel = Vector2::ZERO;
            v.facing = -g.facing;
            g.grab_timer -= 1; // no mash term: a command grab is not shakeable
            if g.grab_timer <= 0 {
                let b = crate::v1::attack_for(t, CharState::SpecialU, g.special_started_air).unwrap().boxes[0];
                // Fixed-angle launch through the shared `strike` ritual: the victim's % still scales
                // the SPEED via the normal knockback formula, but the ANGLE is the box's, mirrored by
                // the attacker's facing. We deliberately do NOT apply victim DI here (`resolve_combat`
                // is where DI bends a trajectory) -- an un-DI-able fixed angle is the "actual Captain
                // Falcon" behavior.
                strike(
                    v,
                    &Swing {
                        hb: &b,
                        dmg: b.damage,
                        kb_scale: 1.0,
                        hitlag_bonus: 4,
                        interrupt: true,
                        aim: Aim::Angle {
                            deg: b.angle,
                            facing: g.facing,
                        },
                    },
                    v.pos,
                    t,
                );
                v.ground_plat = -1;
                v.grab_link = -1;
                v.grab_timer = 0;
                // attacker: a CONNECTED dive is the one up-B ending that is NOT special-fall --
                // in every game this move appears in, landing the grab flips you away actionable
                // with your recovery restored. Air (not Helpless), an airjump-height flyaway
                // hop away from the facing, and the air jump back. The WHIFF path (SpecialU
                // running out with no catch) still falls to Helpless in `za_warudo`.
                g.state = CharState::Air;
                g.frame = 0;
                g.grab_link = -1;
                g.grab_timer = 0;
                g.dive_latch = false;
                g.vel = Vector2::new(-g.facing * CMD_FLYAWAY_X, t.airjump_v);
                g.air_jumps = t.max_air_jumps as u8;
                g.air_dodges = t.max_air_dodges as u8;
                return Some(g.pos); // the shell paints the cheesy fireball at the pair
            }
            return None;
        }
        // slave the victim to the grabber's front, facing back at them. The slave pos can hang
        // past a platform edge the victim was standing on -- clear its ground refs like the
        // dive latch does, or the stale "grounded" read trips `on_real_floor`'s drift assert.
        v.state = CharState::Grabbed;
        v.pos = Vector2::new(g.pos.x + g.facing * GRAB_HELD_X, g.pos.y);
        v.ground_plat = -1;
        v.ground_ink = -1;
        v.vel = Vector2::ZERO;
        v.facing = -g.facing;

        // victim mashes out: every fresh input chips the hold down faster.
        g.grab_timer -= 1 + mash_count(v_in) * t.grab_mash;

        // grabber intents: a fresh stick deflection past the grace throws (direction alone picks
        // fthrow/bthrow/uthrow/dthrow, no button); tap attack = pummel; shield = let go. The grace
        // ignores the stick for GRAB_HOLD_GRACE frames after the catch so the run-in stick that was
        // still held at the moment of the grab can't instant-fire a throw.
        if g.frame > GRAB_HOLD_GRACE && stick_deflected(g_in) {
            do_throw(g, v, g_in, t);
        } else if g_in.shield_pressed {
            release_grab(g, v);
        } else {
            if g_in.attack {
                v.damage += t.pummel_damage;
                g.grab_timer = (g.grab_timer + t.pummel_bonus).min(t.grab_hold);
            }
            if g.grab_timer <= 0 {
                release_grab(g, v); // mashed free / timed out
            }
        }
    }
    None
}

/// Directional throw pick from a stick vector (grab release): the vertical axis dominates when it's
/// bigger, else forward/back by facing. Throw-domain helper (moved out of za_warudo for its budget).
pub(crate) fn throw_dir_from(v: Vector2, facing: f32) -> ThrowDir {
    if v.y.abs() > v.x.abs() {
        if v.y < 0.0 {
            ThrowDir::Up
        } else {
            ThrowDir::Down
        }
    } else if sign(v.x) == facing {
        ThrowDir::Forward
    } else {
        ThrowDir::Back
    }
}

/// Item-throw direction from raw input (held-item toss): deadzoned so a neutral stick is a gentle toss.
pub(crate) fn item_throw_dir(i: &InputFrame, facing: f32) -> Option<ThrowDir> {
    if i.aim_y <= -0.4 {
        Some(ThrowDir::Up)
    } else if i.aim_y >= 0.4 {
        Some(ThrowDir::Down)
    } else if i.dir.abs() >= 0.4 {
        if sign(i.dir) == facing {
            Some(ThrowDir::Forward)
        } else {
            Some(ThrowDir::Back)
        }
    } else {
        None // neutral: gentle toss
    }
}
