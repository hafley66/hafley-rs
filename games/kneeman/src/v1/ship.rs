//! The parked ship (plans/lovers-ship.md): who's steering this frame (helm aim + throttle) and
//! the booster exhaust knockback volume. Split out of `lib.rs` (R5).

use crate::v1::{CharState, DT, InputFrame, SimState, Tune, Vector2, hurtbox, stage};

/// Helm: the hull's ONE station (plans/lovers-ship.md "v3 simplification"), not the hull surface
/// -- only the fighter SEATED there (`Fighter.station >= 0`; occupying is `station::occupy`, gated
/// on interact-in-reach) pilots this frame. A groundling merely standing on the hull is not seated
/// and never steers (keeps smashes instead, the za_warudo Strong lane). Attack = boost: the
/// c-stick is AIM ONLY now (sticky -- survives idle frames, the engine marker stays where you left
/// it), and thrust is 1.0 while the seated pilot holds attack, 0.0 otherwise (thrust does not
/// survive idle frames, unlike aim). Runs before the FSM phase so the exhaust below sees this
/// frame's steering. At most one fighter can ever be seated (one station, `occupy`'s one-rider
/// check), so the first stationed rider found is THE pilot -- no priority tie-break needed.
// parity(v1-ship-steering): steer_from_riders reads the mounted pilot's aim and attack-held throttle
pub(crate) fn steer_from_riders(n: &mut SimState, np: usize, inputs: &[&InputFrame]) {
    n.helm.thrust = 0.0;
    for p in 0..np {
        if n.fighters[p].station < 0 {
            continue;
        }
        let c = Vector2::new(inputs[p].cx, inputs[p].cy);
        let len = c.length();
        if len >= 0.4 {
            n.helm.aim = c / len; // aim only: sticky, no thrust from the stick anymore
        }
        if inputs[p].attack || inputs[p].attack_held {
            n.helm.thrust = 1.0; // attack IS the boost while seated
        }
        break;
    }
}

/// The engine exhaust is a knockback volume: an arc sector off the rim opposite `helm.aim`
/// (the flame is at the BACK — steer right, fire blows left). Any fighter caught in it while
/// the engine burns is launched down the exhaust axis. `hitstun > 0` skips give it a pulse
/// cadence instead of a stunlock; dodges (intangible) sail through. The reaction to this same
/// impulse, along `helm.aim` (opposite the flame), is what actually flies the hull (plans/
/// body-unify.md step 6) — the engine really is "just knockback", both directions of it.
// parity(v1-ship-exhaust-thrust): booster_blast applies hull reaction thrust and the outward exhaust launch volume
pub(crate) fn booster_blast(n: &mut SimState, t: &Tune) {
    if n.helm.thrust <= 0.0 {
        return;
    }
    if n.paths[stage::SHIP_SLOT].len == 0 {
        return;
    }
    // Newton's third law, written directly: `ship_thrust_accel` (px/s^2) is a row/const, applied
    // to the hull's own `vel` every frame the engine burns, converted to ink-native px/frame the
    // same way `integrate_ink` converts gravity (`* DT * DT`). The hull's `gravity_scale` row is
    // 0 (zero-g), so this is the only force that ever moves it -- no hover, no counter-gravity.
    n.paths[stage::SHIP_SLOT].vel += n.helm.aim * (t.ship_thrust_accel * n.helm.thrust * DT * DT);
    let center = n.paths[stage::SHIP_SLOT].pos;
    let ex = -n.helm.aim; // exhaust axis, unit
    for p in 0..(n.active as usize) {
        let f = &mut n.fighters[p];
        if f.hitstun > 0 || f.hitlag > 0 || f.intangible || f.invuln > 0 {
            continue;
        }
        // Crew CONTAINED by the hull (grounded on it, e.g. walking the bowl floor near the rim)
        // and the seated pilot never eat their own ship's exhaust (plans/ship-containment.md #3,
        // 2026-07-06 playtest: a body just standing/riding the hull could get caught in the
        // annulus band and instantly launched with zero input of its own -- "weird force field").
        // An approaching body from OUTSIDE, not yet contained, is still a real hazard.
        if f.station >= 0 || f.ground_ink == stage::SHIP_SLOT as i8 {
            continue;
        }
        let (c, r) = hurtbox(f);
        let v = c - center;
        let d = v.length();
        // inside the annulus [rim, rim + reach] (hurtbox radius counts) and within the
        // sector: v·ex >= cos(half-angle) · |v|, no trig at runtime.
        if d + r < stage::SHIP_R || d - r > stage::SHIP_R + t.booster_len {
            continue;
        }
        if v.dot(ex) < t.booster_cos * d {
            continue;
        }
        f.vel = ex * (t.booster_kb * n.helm.thrust);
        f.hitstun = t.booster_stun;
        f.tumble = false;
        f.state = CharState::Launched;
        f.frame = 0;
    }
}
