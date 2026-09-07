//! `reduce_next_state` -- the per-fighter state machine. One pure step: read the input buffer, run
//! the `CharState` transition table, integrate velocity, resolve stage collision. Cross-fighter
//! combat is NOT here (that is `resolve_combat` in `lib`). Named for the frame-freeze: time stops,
//! every fighter is re-derived, then the world moves again. ZA WARUDO.

use crate::v1::body::{
    Soup, apply_ink_containment, assert_grounded_continuity, on_real_floor, path_surface_vel,
    set_ground, sweep_floors, sweep_walls, touch_refresh,
};
use crate::v1::geo;
use crate::v1::{
    Act, Action, Badge, CharState, DASH_THRESH, DT, DUMMY_FRICTION, ECB_HALF_H, ECB_HALF_W,
    Fighter, GROUND_Y, InkNode, InkPath, InputFrame, Item, KNOCKDOWN_LOCK, Lane, MAX_DRAWN,
    MAX_ITEMS, MAX_PLAYERS, PLATFORMS, STOP_EPS, Tune, Vector2, WALK_THRESH, WALL_TILT_FRAMES,
    ZoneRect, aerial_for, air_drift, airborne, attack_for, do_airdodge, dodge_aim,
    ink_floor_y_near, is_special, item_throw_dir, ledge_drop_opt_out, move_toward, nearest_pickup,
    out_of_zone, respawn, run_special, sign, throw_dir_from, try_ledge_snap, try_special,
};

/// Downhill pull (px/s² of horizontal accel) applied while grounded on a sloped ink Floor whose tilt
/// exceeds the stroke's `floor_tol`. Flat floors (tilt ≤ floor_tol) don't slide, so you just stand.
const SLOPE_SLIDE_ACCEL: f32 = 900.0;

/// Largest vertical jump the grounded-ink pin will follow in one frame (px). A curved hull surface
/// (the ship) steps only a few px per frame while you walk it; a discontinuity larger than this is
/// NOT the same surface — the nearest Floor under the new x belongs to the far side of a closed
/// stroke (the ship hull). Bounding the step kills the ship-top teleport (plans/ac-ship-backlog.md
/// item 2): a big DROP means you walked off the rim over the cockpit opening (fall in with gravity),
/// a big RISE means the far dome/rim is beyond a wall (don't climb onto it — let the wall block x).
pub(crate) const INK_STEP_MAX: f32 = 64.0;

/// Frames an airborne, empty-handed GRAB press stays live as an item-catch latch
/// (`Fighter.catch_win`): a thrown in-flight item arriving within this many frames of the press
/// is caught (update_items' catch pass) instead of hitting. Same forgiving order as the
/// walljump window -- a latch, not a buffered action; spent (zeroed) by the catch itself.
const CATCH_WINDOW: i64 = 8;

/// Footstool feet band around the victim's head: feet at most this far ABOVE the head crown...
const FOOTSTOOL_ABOVE: f32 = 40.0;
/// ...and at most this far below it (sunk into the skull mid-overlap) still count as "on the head".
const FOOTSTOOL_BELOW: f32 = 30.0;

/// Cross-cutting reads for one fighter's frame: derived once from (Fighter, InputFrame, held
/// item) at the top of `reduce_next_state`, before anything ages the buffer or records an edge.
/// Every field here used to be a floating local re-read by branches pages apart (step-slices.md
/// §intent-lanes) -- the c-stick claimants (held item / helm / AC arm / Strong-Aerial macro) all
/// gate on the same handful of derived bits. Reifying them kills the "which local am I allowed to
/// touch" hazard without changing a single computed value: plain data, no mutation, no methods.
#[derive(Clone, Copy)]
struct Reads {
    /// stick-direction sign: -1 / 0 / 1 (`sign(i.dir)`)
    sgn: f32,
    /// stick-direction magnitude, 0..1 (`i.dir.abs()`)
    mag: f32,
    /// main-stick aim vector (i.dir, i.aim_y)
    aim: Vector2,
    /// raw c-stick vector (i.cx, i.cy)
    c: Vector2,
    /// c-stick past the deflection threshold this frame
    c_deflected: bool,
    /// rising edge of `c_deflected` (fresh deflection, not held over from last frame)
    c_edge: bool,
    /// held item is an aiming item -- it claims the c-stick as its gun sight
    aims_held: bool,
    /// empty-handed with an AC core badge -- c-stick is the arm-gun trigger
    ac_armed: bool,
}

impl Reads {
    /// One derivation per fighter per frame. Pure: reads `n`/`i`/`items`, mutates nothing. Safe
    /// to compute before the buffer-aging/record calls below -- none of these fields depend on
    /// anything those calls touch (`n.holding`, `n.ground_ink`, badges, `n.cstick_held` are all
    /// frame-stable until `reduce_next_state`'s own item-intent block runs later this same call).
    fn derive(n: &Fighter, i: &InputFrame, items: &[Item; MAX_ITEMS]) -> Reads {
        let c = Vector2::new(i.cx, i.cy);
        let c_deflected = c.length() >= 0.4;
        Reads {
            sgn: sign(i.dir),
            mag: i.dir.abs(),
            aim: Vector2::new(i.dir, i.aim_y),
            c,
            c_deflected,
            c_edge: c_deflected && !n.cstick_held,
            aims_held: n.holding >= 0
                && crate::v1::item::item_logic(items[n.holding as usize].kind).aims,
            ac_armed: n.holding < 0 && n.has_badge(Badge::AcCore),
        }
    }
}

/// Advance ONE fighter by one frame from its own input: buffer, state machine, integrate +
/// stage collision. No cross-fighter combat (that is `resolve_combat`). Mutates in place.
pub(crate) fn reduce_next_state(
    f: &mut Fighter,
    items: &[Item; MAX_ITEMS],
    paths: &[InkPath; MAX_DRAWN],
    nodes: &[InkNode],
    foes: &[Option<(Vector2, f32)>; MAX_PLAYERS],
    i: &InputFrame,
    t: &Tune,
    zone: Option<ZoneRect>,
) -> Act {
    let mut n = *f;
    let prev = n.state;
    let r = Reads::derive(&n, i, items);

    // impact freeze: on a connect a fighter holds for a few frames (hit "pop"). Nothing
    // advances during hitlag — not the frame timer, not motion, not the buffer.
    if n.hitlag > 0 {
        n.hitlag -= 1;
        *f = n;
        return Act::None;
    }

    // held (grabber or victim): freeze the FSM entirely. `resolve_grab` (cross-fighter) owns the
    // hold: it repositions the victim, runs pummel/throw/mash, and releases. If the link is gone
    // (released this frame), fall back to neutral and let the normal machine resume.
    if matches!(n.state, CharState::GrabHold | CharState::Grabbed) {
        if n.grab_link < 0 {
            n.state = CharState::Stand;
            n.frame = 0;
            *f = n;
            return Act::None;
        }
        n.vel = Vector2::ZERO;
        n.frame += 1;
        *f = n;
        return Act::None;
    }
    // stationed: freeze + pin to anchor (like `Grabbed`); above launch so a tagged rider frees.
    if n.station >= 0 {
        crate::v1::station::stationed_step(&mut n, i, paths, t); // pin to the anchor, or release on jump
        *f = n;
        return Act::None;
    }
    // launched: skip the state machine, run the knockback slide (see `hitstun_slide`'s doc).
    if n.hitstun > 0 {
        hitstun_slide(f, n, paths, nodes, i, t, r.sgn, r.mag, zone);
        return Act::None;
    }

    if n.regrab_lock > 0 {
        n.regrab_lock -= 1;
    }

    // ── input buffer (part 1): age every lane once, refresh the movement-lane diagonal, and record
    // the edges that don't depend on item context (movement + grab). Lanes coexist; within the
    // movement lane the newest of jump / short-hop / air-dodge wins. ──
    for s in &mut n.buf {
        if s.timer > 0 {
            s.timer -= 1;
        }
    }
    n.tick_hit_cd(); // age the per-box re-hit grid once per active (non-frozen) frame
    if n.coyote > 0 {
        n.coyote -= 1;
    }
    if n.invuln > 0 {
        n.invuln -= 1;
    }
    if n.wall_touch > 0 {
        n.wall_touch -= 1;
    }
    if n.arm_cd > 0 {
        n.arm_cd -= 1; // AC arm-gun cadence
    }
    if n.qb_cd > 0 {
        n.qb_cd -= 1; // AC quick-boost cooldown
    }
    if n.catch_win > 0 {
        n.catch_win -= 1; // air-catch latch (armed on the grab record below)
    }
    // shield regenerates whenever the guard is down. Never during ShieldBreak's dizzy — that
    // restores in one piece when the stagger ends (the ShieldBreak arm).
    if !matches!(n.state, CharState::Shield | CharState::ShieldBreak) {
        n.shield_hp = (n.shield_hp + t.shield_regen).min(t.shield_max);
    }
    {
        let m = &mut n.buf[Lane::Movement as usize];
        if m.timer > 0 && r.aim.length() > 0.3 {
            m.aim = r.aim; // latest non-neutral aim within the window wins (the diagonal)
        }
    }
    if i.grab {
        n.record(Lane::Grab, Action::Grab, r.aim, t);
        if n.holding >= 0 {
            // held-item throw gate: Grab's window is 0 (fighter grabs don't buffer), but a
            // throw press stays live for the plat-drop window so a direction flicked just
            // after the button still reads as a directional throw (see the intents block).
            n.buf[Lane::Grab as usize].timer = t.plat_drop_window.max(1) + 1;
        } else if airborne(n.state) {
            // airborne + empty-handed: the same press arms the item-catch latch, whether or
            // not a ground pickup was also in reach (the intents block decides that part).
            n.catch_win = CATCH_WINDOW;
        }
    }
    if i.special {
        n.record(Lane::Special, Action::Special, r.aim, t);
    }
    if i.shorthop {
        n.record(Lane::Movement, Action::ShortHop, r.aim, t);
    } else if i.jump {
        n.record(Lane::Movement, Action::Jump, r.aim, t);
    }
    // shield press only buffers an air dodge when airborne or mid-jumpsquat (else it's a shield)
    if i.shield_pressed && (airborne(n.state) || n.state == CharState::JumpSquat) {
        n.record(Lane::Movement, Action::AirDodge, r.aim, t);
    }

    // ── smash-input flick memory: age the flick counters, refresh them on a fresh crossing into
    // the hard zone. An attack press while an age is fresh (<= t.smash_window) reads as a smash;
    // a held direction reads as a tilt. Works for analog flicks AND digital taps (a keyboard key
    // going down IS a crossing). ──
    let hard_x = r.mag >= DASH_THRESH;
    let hard_y = i.aim_y.abs() >= DASH_THRESH;
    n.flick_x_age = if hard_x && !n.stick_was_hard_x {
        0
    } else {
        n.flick_x_age.saturating_add(1)
    };
    n.flick_y_age = if hard_y && !n.stick_was_hard_y {
        0
    } else {
        n.flick_y_age.saturating_add(1)
    };
    n.stick_was_hard_x = hard_x;
    n.stick_was_hard_y = hard_y;

    // ── c-stick: a deflection edge is an attack macro carrying its own aim. In the air it queues
    // the aerial (main stick stays free for drift — the PM c-stick promise); on the ground it queues
    // a Strong (smash in the flicked direction). Held deflection doesn't re-trigger.
    // holding an aiming item routes the c-stick to AIM: no Strong/Aerial reads while
    // armed (plans/body-bus.md step 7) — the stick is the gun sight, not a macro.
    // holding ANY item, the c-stick is never a smash macro: aiming items use it as the
    // sight, everything else c-flicks a THROW in that direction (Smash smash-throw).
    // a SEATED station pilot never reaches this arm at all -- `n.station >= 0` freezes the
    // whole FSM into `stationed_step` above, before the buffer/lane code below ever runs, so
    // a groundling merely standing on the hull (not seated) keeps smashes (plans/lovers-ship.md
    // "v3 simplification": the gate is occupancy, not standing).
    // an attached AC core claims the empty-handed c-stick as its arm-gun trigger (a held
    // item's own c-stick semantics still win — hands beat shoulders).
    if r.c_edge && n.holding < 0 && !r.ac_armed {
        if airborne(n.state) || n.state == CharState::JumpSquat {
            n.record(Lane::Aerial, Action::Aerial, r.c, t);
        } else {
            n.record(Lane::Strong, Action::Strong, r.c, t);
        }
    }
    n.cstick_held = r.c_deflected;

    // ── item intents: emit a pure descriptor; do NOT mutate the input. `reduce_next_state` only reads `items`
    // (to know if an attack should grab vs jab); `apply_act` does the mutation. ──
    //   holding: grab=drop, attack(held)=fire (full-auto, weaker when held not freshly tapped)
    //   empty + grounded over an item: attack=pickup; else attack=jab/aerial as normal
    //   empty + airborne over an item: GRAB (not attack) picks it up -- attack must stay the
    //   aerial (nearest_pickup's own grounded gate is the reason; see its doc comment), so the
    //   airborne case gets its own ungated reach probe and its own act-arm below, keyed on the
    //   GRAB button only.
    let holding = n.holding >= 0;
    let pickup_target = if holding {
        None
    } else {
        nearest_pickup(&n, items, t)
    };
    let air_pickup_target = if holding || !airborne(n.state) {
        None
    } else {
        crate::v1::item::nearest_pickup_reach(&n, items, t)
    };
    let grab = n.live(Lane::Grab) == Action::Grab;
    let mut act = Act::None;
    let held_draws = holding && crate::v1::item::item_logic(items[n.holding as usize].kind).draws;
    // the attack press that claimed a pickup stays latched until released: no auto-fire/draw
    // on the same press (Smash pickup feel). A grab-button pickup clears next frame.
    if n.pickup_hold && !i.attack && !i.attack_held {
        n.pickup_hold = false;
    }
    if holding {
        if grab {
            // throw gate (the plat-drop-window pattern): a grab press WITH a direction throws
            // now; a neutral grab keeps the buffered window open, so a stick or c-stick flick
            // landing within it still converts to a directional throw — the up-throw tap
            // doesn't need frame-perfect stick+button alignment. Only a window that expires
            // still-neutral is the gentle toss (Drop).
            let expiring = n.buf[Lane::Grab as usize].timer == 1;
            act = match item_throw_dir(i, n.facing) {
                Some(dir) => Act::Throw { dir },
                // (aiming guns hold the c-stick as their SIGHT — that must not convert
                // a neutral drop into a throw)
                None if r.c_deflected && !r.aims_held => Act::Throw {
                    dir: throw_dir_from(r.c, n.facing),
                },
                None if expiring => Act::Drop,
                None => Act::None, // window still open: wait for a flick
            };
            if matches!(act, Act::Throw { .. }) {
                n.clear_lane(Lane::Grab); // spent: don't re-fire or leak into a pickup
            }
        } else if r.c_edge && !r.aims_held {
            // c-flick with a non-aiming item: the smash throw. No turnaround, no dash — the
            // main stick stays free for movement.
            act = Act::Throw {
                dir: throw_dir_from(r.c, n.facing),
            };
        } else if held_draws && i.attack && !n.pickup_hold {
            act = Act::Draw; // paint TOGGLE press (start/stop a stroke) — never a jab
        } else if !held_draws && (i.attack || i.attack_held) && !n.pickup_hold {
            act = Act::Fire {
                auto: i.attack_held && !i.attack,
                // deflected c-stick on an aiming gun = the shot direction; else facing
                aim: if r.aims_held && r.c_deflected {
                    r.c.normalize_or_zero()
                } else {
                    Vector2::ZERO
                },
                aim_y: i.aim_y, // main stick, independent of the c-stick aim above
            };
        }
    } else if pickup_target.is_some() && (i.attack || grab || n.state == CharState::DashAttack) {
        // item grab: attack OR the grab button claims an item you're standing over (empty-handed).
        // The grab press routes here instead of a fighter-grab whenever there's an item to take.
        // A running DashAttack scoops hands-free (Melee): an attack pressed a beat before the item
        // is in reach starts the dash attack, and the slide claims it on the pass-over frame.
        act = Act::Pickup;
    } else if air_pickup_target.is_some() && grab {
        // airborne item grab: GRAB only -- ATTACK stays the aerial (that's the whole point of
        // nearest_pickup's grounded gate; pickup_target is None here so this arm can't fire on
        // an attack press no matter what). No fighter-grab exists in the air today, so this can't
        // collide with one; `grabbing` (below) still keys off `Act::Pickup` either way.
        act = Act::Pickup;
    } else if let Some(a) = crate::v1::station::occupy_intent(&n, items, t, grab) {
        act = a; // empty-handed GRAB over a mounted station: OCCUPY it (in `grabbing`, so no jab)
    }
    // footstool: an airborne jump press directly above another body converts to a head-hop.
    // Decided HERE (not the Air arm) so the movement lane is consumed before the double-jump
    // read sees it; actuation is cross-fighter, so only the descriptor leaves. A live coyote
    // window keeps priority — walking off a lip over someone's head is still your real jump.
    if act == Act::None
        && n.state == CharState::Air
        && n.coyote == 0
        && matches!(n.live(Lane::Movement), Action::Jump | Action::ShortHop)
    {
        if let Some(v) = footstool_target(&n, foes) {
            n.clear_lane(Lane::Movement);
            act = Act::Footstool { victim: v };
        }
    }
    // AC arm gun: continuous fire while the c-stick stays deflected, at the weapon's cadence
    // (`arm_cd` set by ac_fire, ticked below with the other timers). Every state that can act
    // shoots — a mech strafes and fires; only stun/grab shut the trigger off.
    if act == Act::None
        && r.ac_armed
        && r.c_deflected
        && n.arm_cd == 0
        && n.hitstun == 0
        && n.state != CharState::Grabbed
    {
        act = Act::ArmFire {
            aim: r.c.normalize_or_zero(),
        };
    }
    let fire = matches!(act, Act::Fire { .. });
    let grabbing = matches!(
        act,
        Act::Pickup | Act::Drop | Act::Throw { .. } | Act::Occupy { .. }
    );
    let drawing_act = matches!(act, Act::Draw);
    let atk = i.attack && !fire && !grabbing && !drawing_act; // effective attack for jab/aerial
    // grab button with empty hands + no item interaction = a fighter-grab attempt (grounded only).
    let grab_now = grab && !grabbing && !holding;

    // ── input buffer (part 2): the attack edge, now that item context (fire/grab) is resolved.
    // Aerial and Attack are separate lanes, so a jump+attack combo holds both at once (the
    // auto-short-hop macro). Queue an aerial when airborne, mid-jumpsquat, or pressed together with
    // a jump; a grounded attack alone queues a jab that fires on the next actionable ground frame. ──
    let jumping_now = i.jump || i.shorthop;
    if atk {
        if airborne(n.state) || n.state == CharState::JumpSquat || jumping_now {
            n.record(Lane::Aerial, Action::Aerial, r.aim, t);
        } else {
            n.record(Lane::Attack, Action::Attack, r.aim, t);
        }
    }

    let wall_ride = crate::v1::body::cling_wall_vel(&n, paths);
    let force_reset = transition(
        &mut n, i, t, r.sgn, r.mag, atk, grab_now, wall_ride, paths, nodes,
    );

    // ── integrate + collide ─────────────────────────────────────────────────
    let landing_frame = integrate_collide(&mut n, f.pos, paths, nodes, i, t, r.sgn, prev);
    if landing_frame == Some(0) && crate::v1::special_landing_slot(n.state).is_some() {
        n.arm_hits();
    }

    // blast zone -> respawn. `zone` None = ZoneMode::Off; a zone_exempt char never KOs here.
    if zone.is_some_and(|z| !t.zone_exempt && out_of_zone(n.pos, &z)) {
        *f = respawn(&n, t);
        return Act::None;
    }

    // frame counter resets on transition (or a forced re-enter), else advances
    n.frame = if let Some(frame) = landing_frame {
        frame
    } else if n.state != prev || force_reset {
        0
    } else {
        n.frame + 1
    };

    // i-frames drive the debug color
    n.intangible = match n.state {
        CharState::SpotDodge | CharState::Roll | CharState::AirDodge => true,
        CharState::TechInPlace | CharState::TechRoll => true, // teching is fully intangible
        CharState::TechWall => true,                          // wall tech: intangible while stuck
        CharState::LedgeRoll => true,                         // ledge roll = a tech roll's safety
        CharState::Getup => n.frame < t.tech_intang,          // getup i-frames taper off
        CharState::LedgeHold => n.frame < t.ledge_intang,
        // getup swings: safe through the rise, live once the box is out
        CharState::LedgeAttack => n.frame < t.ledge_attack.startup,
        CharState::GetupAttack => n.frame < t.getup_attack.startup,
        _ => false,
    };
    *f = n;
    act
}
/// Launched knockback slide (the old training-dummy physics): friction bleeds horizontal,
/// gravity arcs it down, walls tech/bounce, the floor techs/bounces/knocks down, hitstun ticks.
/// Owns the writeback: every exit assigns `*f`. Split out of `reduce_next_state`.
/// Launched bodies sweep the SAME surface soup as everyone else (body-bus): platforms,
/// drawn ink floors and walls are all real mid-launch, not just the main stage.
fn hitstun_slide(
    f: &mut Fighter,
    mut n: Fighter,
    paths: &[InkPath; MAX_DRAWN],
    nodes: &[InkNode],
    i: &InputFrame,
    t: &Tune,
    sgn: f32,
    mag: f32,
    zone: Option<ZoneRect>,
) {
    let soup = Soup::collect_scoped(paths, nodes, n.pos); // scoped to the hull's own ink if contained
    // tech buffer: a shield press during hitstun arms a tech for `tech_window` frames.
    if n.tech_buf > 0 {
        n.tech_buf -= 1;
    }
    if i.shield_pressed {
        n.tech_buf = t.tech_window as u8;
    }
    let prev = n.pos;
    n.pos += n.vel * DT;
    n.vel.x = move_toward(n.vel.x, 0.0, DUMMY_FRICTION * DT);
    let mut landed = false;
    let mut impact_vy = 0.0; // descending speed at floor contact, captured before the clamp zeroes it
    // floor: the crossed-from-above sweep (no drop-through: hitstun has no drop intent).
    // No floor under us (off the edge, or airborne): arc back down toward the blast zone.
    let floor = if n.vel.y >= 0.0 {
        sweep_floors(prev, n.pos, false, soup.surfs())
    } else {
        None
    };
    match &floor {
        Some(h) => {
            if prev.y < h.y - 0.5 {
                landed = true; // crossed into the floor this frame
                impact_vy = n.vel.y; // remember the spike speed for the floor-bounce check below
            }
            n.pos.x = h.x;
            n.pos.y = h.y;
            n.vel.y = 0.0;
            n.vel += h.vel; // rider inherit (the surf-vel seam, plans/body-unify.md step 4)
            set_ground(&mut n, h.owner);
            touch_refresh(&mut n, t); // grounded contact, any surface
            n.cling_used = 0; // landed: fresh airtime cling budget
        }
        None => {
            n.ground_plat = -1;
            n.ground_ink = -1;
            // hitstun does not gate a ledge catch: launched past the lip still snaps, and
            // the catch IS the recovery -- this mechanic consumes the stun (override layer).
            // The directional zone (plans/ledge-ship-fixes.md #1/#3) is what keeps this from
            // catching a plain crew drop through the hull's hatch: approaching from above rejects.
            if try_ledge_snap(&mut n, t, paths, nodes, sgn) {
                n.hitstun = 0;
                n.tumble = false;
                n.wall_hit = 0;
                *f = n;
                return;
            }
            n.vel.y += t.gravity * DT;
        }
    }

    // launched into a wall — stage face or drawn ink, same rows: tech it (same 20f window
    // as the floor, PM/Ultimate-style) or bounce off with `wall_bounce` restitution.
    // Bounce arms a short tilt window the shell reads. Wall touch refreshes air resources.
    if let Some(w) = sweep_walls(prev.x, n.pos, ECB_HALF_W, ECB_HALF_H, soup.surfs()) {
        n.pos.x = w.x;
        touch_refresh(&mut n, t);
        if n.vel.x * w.nx < 0.0 {
            if n.tumble && n.tech_buf > 0 {
                // wall tech: kill the launch, stick the landing, intangible recovery in place.
                n.tech_buf = 0;
                n.hitstun = 0;
                n.tumble = false;
                n.wall_hit = 0;
                n.vel = Vector2::ZERO;
                n.intangible = true;
                n.frame = 0;
                n.state = CharState::TechWall; // airborne wall tech, NOT the grounded floor-tech
                *f = n;
                return;
            }
            // TUMBLE bounces (wall's own `w.restitution` billiard row or the tumble kick, bigger)
            // and flags the tilt window; a NON-tumble fighter DEAD-STOPS (e=0) so PEN's 0.4 billiard
            // bounce can't leak in and repel it out of the cling/walljump window (drop-test).
            let e = if n.tumble {
                w.restitution.max(t.wall_bounce)
            } else {
                0.0
            };
            n.vel = geo::reflect(n.vel, Vector2::new(w.nx, 0.0), e);
            if n.tumble {
                n.wall_hit = WALL_TILT_FRAMES;
            }
        }
    }

    // ink containment: box-vs-segment SAT resolver (body::apply_ink_containment's doc)
    apply_ink_containment(prev, &mut n, t, ECB_HALF_W, ECB_HALF_H, soup.surfs());
    if n.wall_hit > 0 {
        n.wall_hit -= 1;
    }
    n.hitstun -= 1;

    // spiked into the floor mid-launch: a fast downward tumble (dair stomp) rebounds instead of
    // landing, mirroring the airborne funny-bounce (integrate_collide's landing branch) so an
    // in-match dair stomp bounces the victim off the floor rather than dead-stopping to knockdown.
    if landed && n.tumble && impact_vy > t.tumble_speed {
        n.vel.y = -impact_vy * t.floor_bounce; // still launched + tumbling; hitstun keeps ticking
        n.ground_plat = -1;
        n.ground_ink = -1;
        landed = false; // consumed the landing as a bounce; skip the tech/knockdown resolution
    }

    // hard launch hitting the floor: tech it (intangible recovery) or eat a knockdown.
    if landed && n.tumble {
        n.hitstun = 0;
        n.tumble = false;
        if let Some(h) = &floor {
            set_ground(&mut n, h.owner); // tech/knockdown on whatever caught you: platform or ink
        }
        n.frame = 0;
        if n.tech_buf > 0 {
            n.tech_buf = 0;
            n.intangible = true; // tech i-frames start immediately (this branch early-returns)
            if sgn != 0.0 && mag >= DASH_THRESH {
                n.facing = sgn;
                n.vel.x = sgn * t.techroll_speed;
                n.state = CharState::TechRoll; // teched with a roll
            } else {
                n.vel.x = 0.0;
                n.state = CharState::TechInPlace; // teched in place
            }
        } else {
            n.vel.x = 0.0;
            n.state = CharState::Knockdown; // missed the tech -> floored
        }
        *f = n;
        return;
    }

    if n.hitstun == 0 {
        // light launch / drifted out: recover normally (on a floor -> Stand, else Air).
        n.tumble = false;
        n.wall_hit = 0;
        match &floor {
            Some(h) => {
                n.state = CharState::Stand;
                set_ground(&mut n, h.owner);
            }
            None => {
                // the save: a live tech buffer fires an air dodge the instant stun expires,
                // spending a charge and cancelling launch momentum. A full reset under
                // save_zero_pct damage; past it the launch bleeds back in linearly
                // (Ultimate-style), so a late save at high % still carries you out.
                // Into the ground a frame later it wavelands like any air dodge.
                if n.tech_buf > 0 && n.air_dodges > 0 {
                    let over = (n.damage - t.save_zero_pct).max(0.0);
                    let residual = n.vel * (over * t.save_scale).min(1.0);
                    n.tech_buf = 0;
                    n.frame = 0;
                    n.ground_plat = -1;
                    do_airdodge(&mut n, Vector2::new(i.dir, i.aim_y), t);
                    n.vel += residual;
                } else {
                    n.state = CharState::Air;
                    n.ground_plat = -1;
                }
            }
        }
    }
    if zone.is_some_and(|z| !t.zone_exempt && out_of_zone(n.pos, &z)) {
        *f = respawn(&n, t);
        return;
    }
    *f = n;
}

/// The `CharState` transition table: one arm per state, mutating `n` toward its next frame.
/// Pure FSM only — integration/collision happen after, in `integrate_collide`. Returns
/// `force_reset` (re-enter same state, e.g. dash-dance, so the frame timer restarts).
#[allow(clippy::too_many_arguments)]
fn transition(
    n: &mut Fighter,
    i: &InputFrame,
    t: &Tune,
    sgn: f32,
    mag: f32,
    atk: bool,
    grab_now: bool,
    wall_ride: Vector2, // armed wall's px/frame translation (body::cling_wall_vel); the cling carry
    paths: &[InkPath; MAX_DRAWN],
    nodes: &[InkNode], // read-only ink geometry for the ledge climb
) -> bool {
    let mut force_reset = false;
    // match on a Copy of the state so arm guards (e.g. try_special) can mutate `n`.
    let cur_state = n.state;
    match cur_state {
        CharState::Stand => {
            if !try_ground_action(n, i, atk, grab_now, t) {
                if i.down {
                    if sgn != 0.0 {
                        n.facing = sgn;
                    }
                    n.state = CharState::Crouch;
                } else if sgn != 0.0 && mag >= DASH_THRESH {
                    n.facing = sgn;
                    n.vel.x = sgn * t.dash_init; // initial dash burst impulse
                    n.state = CharState::Dash;
                } else if sgn != 0.0 && mag >= WALK_THRESH {
                    n.facing = sgn;
                    n.state = CharState::Walk;
                } else {
                    n.vel.x = move_toward(n.vel.x, 0.0, t.ground_friction * DT);
                }
            }
        }
        CharState::Walk => {
            if !try_ground_action(n, i, atk, grab_now, t) {
                if mag < WALK_THRESH {
                    n.state = CharState::Stand;
                } else if sgn != n.facing {
                    n.state = CharState::Turn; // standing pivot
                } else {
                    n.vel.x = move_toward(n.vel.x, sgn * t.walk_speed, t.ground_accel * DT);
                }
            }
        }
        CharState::Dash => {
            if !try_ground_action(n, i, atk, grab_now, t) {
                if sgn != 0.0 && sgn != n.facing && mag >= DASH_THRESH {
                    // dash-dance: flip facing + restart the window, but DON'T teleport velocity.
                    // Old momentum bleeds across 0 in the accel branch below, so a fast wrong-way
                    // flick costs distance/time. No more instant free reversal.
                    n.facing = sgn;
                    force_reset = true;
                } else if mag < WALK_THRESH {
                    n.state = CharState::Skid; // release mid-dash -> slide to a stop (dashstop)
                } else {
                    // fighting your own momentum (vel still points the old way) brakes at
                    // dash_turn_accel; once vel agrees with facing, normal dash accel toward run.
                    let a = if sign(n.vel.x) == -n.facing {
                        t.dash_turn_accel
                    } else {
                        t.ground_accel
                    };
                    n.vel.x = move_toward(n.vel.x, n.facing * t.run_speed, a * DT);
                    if n.frame >= t.dash_window {
                        n.state = CharState::Run;
                    }
                }
            }
        }
        CharState::Run => {
            if !try_ground_action(n, i, atk, grab_now, t) {
                if mag < WALK_THRESH || sgn != n.facing {
                    n.state = CharState::Skid; // release or reverse -> run brake
                } else {
                    n.vel.x = move_toward(n.vel.x, n.facing * t.run_speed, t.ground_accel * DT);
                }
            }
        }
        CharState::Turn => {
            n.vel.x = move_toward(n.vel.x, 0.0, t.ground_friction * DT);
            if !try_ground_action(n, i, atk, grab_now, t) && n.frame >= t.pivot_frames {
                n.facing = -n.facing;
                if sgn != 0.0 && mag >= DASH_THRESH {
                    // standing pivot already bled momentum to ~0 over pivot_frames, so this is a
                    // fresh dash from rest: full initial burst, same as dashing from neutral.
                    n.vel.x = n.facing * t.dash_init;
                    n.state = CharState::Dash;
                } else if sgn != 0.0 && mag >= WALK_THRESH {
                    n.state = CharState::Walk;
                } else {
                    n.state = CharState::Stand;
                }
            }
        }
        CharState::Skid => {
            if !try_ground_action(n, i, atk, grab_now, t) {
                // No instant pivot: the brake always runs the velocity down THROUGH zero first.
                // Only at the zero-velocity point does a held reverse turn into a fresh dash the
                // other way — the Melee run-turnaround, not a free momentum teleport.
                n.vel.x = move_toward(n.vel.x, 0.0, t.dashstop_friction * DT);
                if n.vel.x.abs() < STOP_EPS {
                    n.vel.x = 0.0;
                    if sgn != 0.0 && sgn != n.facing && mag >= DASH_THRESH {
                        n.facing = sgn;
                        n.vel.x = sgn * t.dash_init; // planted, then burst out the other way
                        n.state = CharState::Dash;
                    } else {
                        n.state = CharState::Stand;
                    }
                }
            }
        }
        CharState::Crouch => {
            // hold down to stay crouched; jump/shield available; release down -> stand;
            // down + a direction -> crawl. Bleed any residual run momentum while squatting.
            n.vel.x = move_toward(n.vel.x, 0.0, t.ground_friction * DT);
            if !try_ground_action(n, i, atk, grab_now, t) {
                if !i.down {
                    n.state = CharState::Stand;
                } else if sgn != 0.0 && mag >= WALK_THRESH {
                    n.facing = sgn;
                    n.state = CharState::Crawl;
                }
            }
        }
        CharState::Crawl => {
            // crouched creep: facing follows the stick (crawl both ways by turning). Down is
            // held, so an attack from here reads as the dtilt in try_ground_action. Stick
            // released -> back to the squat; down released -> stand.
            if !try_ground_action(n, i, atk, grab_now, t) {
                if !i.down {
                    n.state = CharState::Stand;
                } else if sgn == 0.0 || mag < WALK_THRESH {
                    n.state = CharState::Crouch;
                } else {
                    n.facing = sgn;
                    n.vel.x = move_toward(n.vel.x, sgn * t.crawl_speed, t.ground_accel * DT);
                }
            }
        }
        CharState::Landing => {
            n.vel.x = move_toward(n.vel.x, 0.0, t.ground_friction * DT);
            if let Some(full) = take_jump(n) {
                n.state = CharState::JumpSquat;
                n.full_hop = full;
            } else if n.frame >= t.landing_lag {
                n.state = CharState::Stand;
            }
        }
        CharState::Shield => {
            // the raised guard drains every frame; empty = break. Incoming hits resolve in
            // `strike` (Guard::Shield): hp eats the damage, shield_stun locks the guard.
            n.shield_hp -= t.shield_decay;
            if n.shield_hp <= 0.0 {
                n.shield_hp = 0.0;
                n.shield_stun = 0;
                n.state = CharState::ShieldBreak;
            } else if n.shield_stun > 0 {
                // guard hitstun from a block: stuck in shield while the pushback slides out.
                n.shield_stun -= 1;
                n.vel.x = move_toward(n.vel.x, 0.0, t.ground_friction * DT);
            } else if let Some(full) = take_jump(n) {
                // jump out of shield, drop shield, roll, or spot dodge
                n.state = CharState::JumpSquat;
                n.full_hop = full;
            } else if !i.shield_held {
                n.state = CharState::Stand;
            } else if sgn != 0.0 && mag >= DASH_THRESH {
                n.facing = sgn;
                n.vel.x = sgn * t.roll_speed;
                n.state = CharState::Roll;
            } else if i.down {
                n.vel.x = 0.0;
                n.state = CharState::SpotDodge;
            } else {
                n.vel.x = move_toward(n.vel.x, 0.0, t.ground_friction * DT);
            }
        }
        CharState::ShieldBreak => {
            // dizzy: fully punishable, far longer than any move's endlag. The shield itself
            // comes back whole when the stagger ends.
            n.vel.x = move_toward(n.vel.x, 0.0, t.ground_friction * DT);
            if n.frame >= t.shieldbreak_frames {
                n.shield_hp = t.shield_max;
                n.state = CharState::Stand;
            }
        }
        CharState::Rebound => {
            // clank recoil: the cancelled swing staggers back, then neutral. No actions —
            // the trade's payoff is the opponent's identical stagger (or their live move).
            n.vel.x = move_toward(n.vel.x, 0.0, t.dashstop_friction * DT);
            if n.frame >= t.rebound_frames {
                n.state = CharState::Stand;
            }
        }
        CharState::SpotDodge => {
            n.vel.x = move_toward(n.vel.x, 0.0, t.ground_friction * DT);
            if n.frame >= t.spotdodge_frames {
                n.state = if i.shield_held {
                    CharState::Shield
                } else {
                    CharState::Stand
                };
            }
        }
        CharState::Roll => {
            // hold the roll velocity, then end (intangible mid-roll via the i-frame window)
            if n.frame >= t.roll_frames {
                n.vel.x = 0.0;
                n.state = if i.shield_held {
                    CharState::Shield
                } else {
                    CharState::Stand
                };
            }
        }
        CharState::JumpSquat => {
            // ground physics keep running during the squat. Hold the dash dir -> accelerate
            // toward run speed (full dash-jump carry); go neutral -> friction bleeds vel.x,
            // so jumping out of a dash-stop transfers little momentum. This is the last
            // actionable window to set direction before the air locks it.
            if sgn != 0.0 {
                n.facing = sgn;
            }
            if sgn == 0.0 {
                n.vel.x = move_toward(n.vel.x, 0.0, t.ground_friction * DT);
            } else {
                n.vel.x = move_toward(n.vel.x, sgn * t.run_speed, t.ground_accel * DT);
            }
            if !i.jump_held && n.full_hop {
                n.full_hop = false; // released before takeoff -> short hop
            }
            // jump-cancel up smash (PM): an attack during the squat with the stick (or c-stick)
            // held up converts the jump into a usmash that KEEPS the slide momentum. This is also
            // how tap-jump stick users reach usmash at all — their up-flick became this squat.
            let squat_aerial = n.live(Lane::Aerial) == Action::Aerial;
            let up_now = i.aim_y <= -0.35 || n.buf[Lane::Aerial as usize].aim.y <= -0.35;
            if grab_now {
                n.clear_lane(Lane::Movement);
                n.clear_lane(Lane::Aerial);
                enter_grab(n);
            } else if squat_aerial && up_now {
                n.clear_lane(Lane::Aerial);
                n.arm_hits();
                n.charge = 0; // jump-cancel usmash banks like any other smash
                n.state = CharState::Usmash;
            } else if n.frame >= t.jumpsquat - 1 {
                let wavedash = n.live(Lane::Movement) == Action::AirDodge;
                if wavedash {
                    let aim = dodge_aim(&n, i);
                    n.clear_lane(Lane::Movement);
                    do_airdodge(n, aim, t); // wavedash: airdodge straight out of jumpsquat
                } else {
                    // jump+attack combo = auto short-hop aerial (Ultimate): force a short hop and
                    // tag the aerial for reduced damage. Set before vel.y so the hop comes out short.
                    if n.live(Lane::Aerial) == Action::Aerial {
                        n.full_hop = false;
                        n.autohop_aerial = true;
                    }
                    n.vel.y = if n.full_hop {
                        t.fullhop_v
                    } else {
                        t.shorthop_v
                    };
                    // keep ground momentum * carry, ADD stick contribution, clamp to a cap
                    // that sits ABOVE run speed so a dash-jump does NOT lose speed.
                    let h = n.vel.x * t.momentum_carry + i.dir * t.jump_h_init;
                    n.vel.x = h.clamp(-t.jump_h_max, t.jump_h_max);
                    n.state = CharState::Air; // air_jumps/dodges already set from ground contact
                }
            }
        }
        CharState::Air if try_special(n) => {
            // entered a special from the air; the SpecialX arm runs from frame 0 next tick
        }
        CharState::Air => {
            let want_dodge = n.live(Lane::Movement) == Action::AirDodge;
            let buffered_aerial = n.live(Lane::Aerial) == Action::Aerial;
            let want_aerial = atk || buffered_aerial;
            if want_aerial {
                // a same-frame press has no captured aim yet; read it live in that case.
                let aim = if buffered_aerial {
                    n.buf[Lane::Aerial as usize].aim
                } else {
                    Vector2::new(i.dir, i.aim_y)
                };
                n.clear_lane(Lane::Aerial);
                n.state = aerial_for(aim, n.facing, t); // 5-way pick, stick relative to facing
                n.arm_hits();
            } else if want_dodge && n.air_dodges > 0 {
                let a = dodge_aim(&n, i);
                n.clear_lane(Lane::Movement);
                do_airdodge(n, a, t); // directional burst; into the ground = wavedash
            } else {
                // double jump: cancels fall (crisp upward pop even while falling fast) and
                // REDIRECTS horizontal from the stick — hold back to reverse momentum.
                let want_djump = matches!(n.live(Lane::Movement), Action::Jump | Action::ShortHop);
                // buttonless walljump + cling (queue-2026-07-03 item 1): a hard deflection away
                // from an armed wall contact kicks with no jump press; into it clings instead,
                // budget-capped so it can't hang forever. Generic wall layer (crate::v1::body), so
                // ink walls get both for free, same as the jump-press kick already did.
                let wall_away = crate::v1::body::wall_deflect_away(n.wall_nx, sgn, mag);
                let wall_into = n.wall_touch > 0
                    && crate::v1::body::wall_cling_incident(n.wall_nx, n.vel, sgn, mag);
                let clinging = wall_into && n.cling_used < t.cling_frames;
                if (want_djump || wall_away) && n.wall_touch > 0 && n.coyote == 0 {
                    // walljump: a fresh wall contact converts the jump. Deflected stick =
                    // the kick; NEUTRAL stick = straight-up hop (body::wall_jump_apply).
                    // No air jump spent; the window is, so re-arming takes a new touch.
                    n.clear_lane(Lane::Movement);
                    n.wall_touch = 0;
                    crate::v1::body::wall_jump_apply(n, mag, t);
                } else if clinging {
                    n.cling_used += 1; // spent by `apply_air_gravity` below
                    n.pos += wall_ride; // moving-hull cling follows the wall (body::cling_wall_vel)
                } else if want_djump && n.coyote > 0 {
                    // coyote jump: walked off the lip a few frames ago, so this is still the
                    // GROUNDED jump (full/short by which lane), instant (no jumpsquat), and it
                    // does NOT spend the air jump. Fixes "lost a jump the instant I left the edge".
                    let full = n.live(Lane::Movement) == Action::Jump;
                    n.clear_lane(Lane::Movement);
                    n.coyote = 0;
                    n.vel.y = if full { t.fullhop_v } else { t.shorthop_v };
                    n.fast_falling = false;
                    let h = n.vel.x * t.momentum_carry + i.dir * t.jump_h_init;
                    n.vel.x = h.clamp(-t.jump_h_max, t.jump_h_max);
                } else if want_djump && n.has_badge(Badge::AcCore) {
                    // AC frame: the jump tap is a QUICK BOOST — a burst toward the stick on a
                    // cooldown — never a double jump (the mech traded them away). Neutral stick
                    // dodges along facing. On cooldown the tap just fizzles (lane still spent,
                    // no jump banked for later).
                    n.clear_lane(Lane::Movement);
                    if n.qb_cd == 0 {
                        let a = Vector2::new(i.dir, i.aim_y);
                        let dir = if a.length() >= 0.3 {
                            a / a.length()
                        } else {
                            Vector2::new(n.facing, 0.0)
                        };
                        n.vel = dir * t.ac_qb_speed;
                        n.fast_falling = false;
                        n.qb_cd = t.ac_qb_cd as u8;
                    }
                } else if want_djump && (n.air_jumps > 0 || n.has_badge(Badge::Wings)) {
                    n.clear_lane(Lane::Movement);
                    if !n.has_badge(Badge::Wings) {
                        n.air_jumps -= 1; // Wings: air jumps never spend (the badge's whole deal)
                    }
                    n.vel.y = t.airjump_v;
                    n.fast_falling = false;
                    if sgn != 0.0 {
                        // momentum redirects with the stick, but facing does NOT flip: an air jump
                        // can't turn you around (only a turnaround special could). Ult-style.
                        let dj = sgn * t.airjump_h;
                        // hold AWAY -> reverse to fresh horizontal; hold TOWARD -> keep your
                        // speed, never slow below airjump_h.
                        n.vel.x = if sign(n.vel.x) != sgn {
                            dj
                        } else {
                            sgn * n.vel.x.abs().max(t.airjump_h.abs())
                        };
                    }
                    // neutral stick: keep current horizontal momentum
                }
                air_drift(n, i, t, sgn);
                if n.has_badge(Badge::AcCore) && i.jump_held {
                    // AC boost: held jump is constant thrust toward the stick (neutral = straight
                    // up), speed-capped. Gravity below still pulls every frame — climbing is
                    // thrust WINNING, not gravity pausing.
                    let a = Vector2::new(i.dir, i.aim_y);
                    let dir = if a.length() >= 0.3 {
                        a / a.length()
                    } else {
                        Vector2::new(0.0, -1.0)
                    };
                    n.vel += dir * t.ac_boost_accel * DT;
                    let sp = n.vel.length();
                    if sp > t.ac_boost_max {
                        n.vel = n.vel / sp * t.ac_boost_max;
                    }
                    n.fast_falling = false;
                }
                // fast fall + gravity, or the wall-cling freeze in its place (queue-2026-07-03
                // item 1) -- split out to body.rs so this arm doesn't regrow za_warudo.rs.
                crate::v1::body::apply_air_gravity(n, i, t, clinging);
            }
        }
        CharState::AirDodge => {
            // burst decays (drag) so an open-air dodge lunges and settles instead of flying;
            // a wavedash lands within a frame or two so its horizontal is still mostly intact.
            n.vel.x = move_toward(n.vel.x, 0.0, t.airdodge_drag * DT);
            n.vel.y = move_toward(n.vel.y, 0.0, t.airdodge_drag * DT);
            if n.frame >= t.airdodge_frames {
                n.vel.y = 0.0;
                n.state = CharState::Air; // actionable again (Ultimate-style, not helpless)
            }
        }
        CharState::LedgeHold => {
            if take_jump(n).is_some() {
                n.vel.y = t.ledgejump_v;
                n.vel.x = n.facing * t.jump_h_init; // hop toward the stage
                n.state = CharState::Air;
                n.regrab_lock = 20;
            } else if atk {
                // ledge getup attack: up onto the lip swinging inward, intangible startup.
                n.arm_hits();
                climb_onto_stage(n, paths, nodes);
                n.state = CharState::LedgeAttack;
            } else if i.shield_held {
                // ledge roll: up and inward at tech-roll speed, intangible the whole way.
                climb_onto_stage(n, paths, nodes);
                n.vel.x = n.facing * t.techroll_speed;
                n.state = CharState::LedgeRoll;
            } else if sgn == n.facing && mag >= WALK_THRESH {
                n.state = CharState::LedgeClimb; // hold toward stage = neutral getup
            } else if (sgn == -n.facing && mag >= WALK_THRESH) || i.down_pressed {
                // away from the stage, or a DELIBERATE down tap — not a held-down from the
                // fast-fall into the grab (that would slip you straight off the lip).
                n.state = CharState::Air; // drop off
                n.regrab_lock = 20;
            }
            // else keep hanging (position is fixed by the integrate block)
        }
        CharState::LedgeClimb => {
            if n.frame >= t.climb_frames {
                climb_onto_stage(n, paths, nodes);
                n.state = CharState::Stand;
            }
        }
        CharState::LedgeRoll => {
            // roll inward off the ledge grab (intangible via the i-frame match), then settle.
            if n.frame >= t.techroll_frames {
                n.vel.x = 0.0;
                n.state = CharState::Stand;
            }
        }
        CharState::Jab => {
            // grounded swing: hard brake to a planted stop, run out the frame data, then neutral.
            n.vel.x = move_toward(n.vel.x, 0.0, t.ground_friction * 3.0 * DT);
            let atk = attack_for(t, CharState::Jab, false).unwrap();
            if n.frame >= atk.total() - 1 {
                n.state = if i.shield_held {
                    CharState::Shield
                } else {
                    CharState::Stand
                };
            }
        }
        CharState::Dtilt => {
            // crouched pothole swing: planted (feet stay put), run the frame data, then back to a
            // crouch if down is still held, else stand. Same brake as a jab.
            n.vel.x = move_toward(n.vel.x, 0.0, t.ground_friction * 3.0 * DT);
            let atk = attack_for(t, CharState::Dtilt, false).unwrap();
            if n.frame >= atk.total() - 1 {
                n.state = if i.down {
                    CharState::Crouch
                } else {
                    CharState::Stand
                };
            }
        }
        CharState::DashAttack => {
            // lunge: slide through the swipe carrying the lunge speed (barely any friction), then
            // brake hard once the endlag starts so the commitment still plants you. No steering.
            let atk = attack_for(t, CharState::DashAttack, false).unwrap();
            let sliding = n.frame < atk.active_end(); // still swinging = the drive; then brake
            let fric = if sliding {
                t.dashstop_friction * 0.12
            } else {
                t.dashstop_friction
            };
            n.vel.x = move_toward(n.vel.x, 0.0, fric * DT);
            if n.frame >= atk.total() - 1 {
                n.state = if i.shield_held {
                    CharState::Shield
                } else {
                    CharState::Stand
                };
            }
        }
        CharState::Grab => {
            // reach planted in place; run startup + active + heavy whiff recovery, then neutral.
            // The catch itself lives in `resolve_grab` (it needs the other fighter); on a catch that
            // flips this fighter to GrabHold before the next frame reaches this arm.
            n.vel.x = move_toward(n.vel.x, 0.0, t.ground_friction * 3.0 * DT);
            let total = t.grab_startup + t.grab_active + t.grab_recovery;
            if n.frame >= total - 1 {
                n.state = if i.shield_held {
                    CharState::Shield
                } else {
                    CharState::Stand
                };
            }
        }
        // held states are early-returned above; arms exist only for match exhaustiveness.
        CharState::GrabHold | CharState::Grabbed => {}
        CharState::Knockdown => {
            // floored: slide to a stop, unactionable briefly, then getup options / auto-getup.
            n.vel.x = move_toward(n.vel.x, 0.0, t.ground_friction * DT);
            if n.frame >= t.knockdown_frames {
                n.state = CharState::Getup; // lay too long -> stand up automatically
            } else if n.frame >= KNOCKDOWN_LOCK {
                if i.attack {
                    n.arm_hits();
                    n.state = CharState::GetupAttack; // rising sweep, both sides
                } else if sgn != 0.0 && mag >= DASH_THRESH {
                    n.facing = sgn;
                    n.vel.x = sgn * t.techroll_speed;
                    n.state = CharState::TechRoll; // getup roll
                } else if i.jump || i.shorthop || i.shield_pressed || i.aim_y <= -0.4 {
                    n.state = CharState::Getup; // neutral getup
                }
            }
        }
        CharState::Getup => {
            n.vel.x = move_toward(n.vel.x, 0.0, t.ground_friction * DT);
            if n.frame >= t.getup_frames {
                n.state = CharState::Stand;
            }
        }
        CharState::TechInPlace => {
            n.vel.x = move_toward(n.vel.x, 0.0, t.ground_friction * 2.0 * DT);
            if n.frame >= t.tech_intang {
                n.state = CharState::Stand;
            }
        }
        CharState::TechWall => {
            // wall tech: stuck to the wall, intangible, frozen in place (vel zeroed at entry, no
            // gravity applied here). The wall sweep re-arms the walljump window each frame, so a jump
            // press escapes; otherwise fall once the tech i-frames expire.
            if n.frame >= t.tech_intang {
                n.state = CharState::Air;
            }
        }
        CharState::TechRoll => {
            // roll across the ground (intangible), then settle to neutral.
            if n.frame >= t.techroll_frames {
                n.vel.x = 0.0;
                n.state = CharState::Stand;
            }
        }
        st @ (CharState::Nair
        | CharState::Fair
        | CharState::Bair
        | CharState::Uair
        | CharState::Dair) => {
            // aerial swing: drift + gravity still apply; ends back to Air (or lands via integrate).
            air_drift(n, i, t, sgn);
            n.vel.y += t.gravity * DT;
            if n.vel.y > t.max_fall {
                n.vel.y = t.max_fall;
            }
            let atk = attack_for(t, st, false).unwrap();
            if n.frame >= atk.total() - 1 {
                n.state = CharState::Air;
                n.autohop_aerial = false;
            }
        }
        st @ (CharState::Ftilt
        | CharState::Utilt
        | CharState::LedgeAttack
        | CharState::GetupAttack) => {
            // planted swing: same hard brake + frame-data runout as a jab. The two getup
            // swings ride the same shape — their intangible startup is the i-frame match.
            n.vel.x = move_toward(n.vel.x, 0.0, t.ground_friction * 3.0 * DT);
            let atk = attack_for(t, st, false).unwrap();
            if n.frame >= atk.total() - 1 {
                n.state = if i.shield_held {
                    CharState::Shield
                } else {
                    CharState::Stand
                };
            }
        }
        st @ (CharState::Fsmash | CharState::Dsmash) => {
            // planted smash: dashstop-grade brake — the commitment is standing still and swinging.
            n.vel.x = move_toward(n.vel.x, 0.0, t.dashstop_friction * DT);
            let atk = attack_for(t, st, false).unwrap();
            if charge_smash(n, i, t) {
                force_reset = true; // charging: the clock stays pinned at 0
            } else if n.frame >= atk.total() - 1 {
                n.state = if i.shield_held {
                    CharState::Shield
                } else {
                    CharState::Stand
                };
            }
        }
        CharState::Usmash => {
            // up smash keeps its slide (PM jump-cancel usmash out of a run): only normal ground
            // friction bleeds the momentum, so a running JC usmash travels through the swing.
            n.vel.x = move_toward(n.vel.x, 0.0, t.ground_friction * DT);
            let atk = attack_for(t, CharState::Usmash, false).unwrap();
            if charge_smash(n, i, t) {
                force_reset = true;
            } else if n.frame >= atk.total() - 1 {
                n.state = if i.shield_held {
                    CharState::Shield
                } else {
                    CharState::Stand
                };
            }
        }
        CharState::SpecialN => run_special(n, 0, i, t),
        CharState::SpecialS => run_special(n, 1, i, t),
        CharState::SpecialU => run_special(n, 2, i, t),
        CharState::SpecialD => run_special(n, 3, i, t),
        CharState::SpecialLandN | CharState::SpecialLandS |
        CharState::SpecialLandU | CharState::SpecialLandD => crate::v1::run_special_landing(n, t),
        CharState::Helpless => {
            // special-fall: drift only, gravity pulls, no actions until you land (integrate -> Landing)
            air_drift(n, i, t, sgn);
            n.vel.y += t.gravity * DT;
            if n.vel.y > t.max_fall {
                n.vel.y = t.max_fall;
            }
        }
        CharState::Launched => {
            // Reached only if a connect set Launched but hitstun floored to 0 (a feather tap): the
            // hitstun branch above never ran, so recover here the same way it exits (air -> Air,
            // grounded -> Stand). Normal launches spend their time in the `hitstun > 0` branch.
            n.tumble = false;
            if n.pos.y < GROUND_Y {
                n.state = CharState::Air;
                n.ground_plat = -1;
            } else {
                n.state = CharState::Stand;
                n.ground_plat = 0;
            }
        }
    }
    force_reset
}

/// Integrate velocity and resolve collision: ledge snap, platform + ink landings, walls,
/// grounded edge-stick, drawn-ink wall blocks. `prev_pos` is the feet position before this
/// frame's motion (platform-crossing tests + wall sweep origin). Split out of `reduce_next_state`.
// parity(v1-ink-fighter-collision-order): the fighter resolves ink floor landing before wall and containment, dead-stops ordinary landings, and applies floor restitution only to fast tumbling impact
fn integrate_collide(
    n: &mut Fighter,
    prev_pos: Vector2,
    paths: &[InkPath; MAX_DRAWN],
    nodes: &[InkNode],
    i: &InputFrame,
    t: &Tune,
    sgn: f32,
    prev_state: CharState, // pre-transition state (`climbed_onto_stage_this_frame`'s doc)
) -> Option<i64> {
    let mut landing_frame = None;
    let soup = Soup::collect_scoped(paths, nodes, n.pos); // scoped to the hull's own ink if contained
    let prev_y = prev_pos.y; // feet-y before this frame's motion (for platform crossing tests)
    // for the continuity invariant below: "grounded" means a ground-pinning branch actually runs
    // this call -- `ground_plat`/`ground_ink` alone can be stale (e.g. a fresh jump leaves them
    // set until the next landing), so airborne/ledge states must win over the raw fields.
    let grounded_before = !airborne(n.state) && !is_ledge(n.state) && n.grounded();
    if airborne(n.state) {
        n.pos += n.vel * DT;

        // ledge snap: falling, in the lip's directional zone, minus the deliberate-drop opt-out
        // (physics::ledge_drop_opt_out, plans/ledge-ship-fixes.md #4).
        if n.state == CharState::Air && !ledge_drop_opt_out(n, i) {
            try_ledge_snap(n, t, paths, nodes, sgn);
        }

        // landing: crossed a floor surf from above while descending — platforms and drawn
        // ink through the SAME sweep (highest crossed surface wins). Soft rows are skipped
        // while holding down (drop-through) — UNLESS this is an air dodge (wavedash),
        // where the down is the dodge aim, not a drop command.
        if airborne(n.state) && n.vel.y >= 0.0 {
            let drop = i.down && n.state != CharState::AirDodge;
            if let Some(hit) = sweep_floors(prev_pos, n.pos, drop, soup.surfs()) {
                // a fast tumbling (spiked) body bounces off the floor instead of landing: invert
                // vel.y scaled by floor_bounce and STAY airborne + tumbling (the dair funny bounce).
                if n.tumble && n.vel.y > t.tumble_speed {
                    n.pos.x = hit.x;
                    n.pos.y = hit.y;
                    n.vel.y = -n.vel.y * t.floor_bounce;
                } else {
                    n.pos.x = hit.x;
                    n.pos.y = hit.y;
                    n.vel.y = 0.0;
                    n.vel += hit.vel; // rider inherit (the surf-vel seam, plans/body-unify.md step 4)
                    n.fast_falling = false;
                    touch_refresh(n, t);
                    n.coyote = 0; // landed: the grace window is spent
                    n.cling_used = 0; // landed: fresh airtime cling budget
                    set_ground(n, hit.owner);
                    (n.state, landing_frame) = crate::v1::land_transition(t, n.state, n.special_started_air);
                }
            }
        }
    } else if is_special(n.state) {
        // specials integrate by where they launched: aerial (ground_plat < 0) falls + lands on a
        // platform top -> Landing; grounded stays pinned (the planted punch). Main floor + soft tops.
        if !n.grounded() {
            n.pos += n.vel * DT;
            // an aerial special lands on platforms and drawn ink alike: same sweep as the
            // airborne branch (no wavedash exception here — down always reads as drop intent).
            if n.vel.y >= 0.0 {
                if let Some(hit) = sweep_floors(prev_pos, n.pos, i.down, soup.surfs()) {
                    n.pos.x = hit.x;
                    n.pos.y = hit.y;
                    n.vel.y = 0.0;
                    n.vel += hit.vel; // rider inherit (the surf-vel seam, plans/body-unify.md step 4)
                    touch_refresh(n, t);
                    n.cling_used = 0; // landed: fresh airtime cling budget
                    set_ground(n, hit.owner);
                    (n.state, landing_frame) = crate::v1::land_transition(t, n.state, n.special_started_air);
                }
            }
        } else if n.on_ink() {
            // special launched while standing on ink: ground_plat reads 0 there (the "grounded"
            // convention from the ink landing), so pin to the INK surface, not PLATFORMS[0]. Same
            // discontinuity hazard as the WALK arm below: `ink_floor_y_at` always hands back the
            // HIGHEST walkable point spanning x, so pressing B just under the ship hull's underside
            // re-derived "the floor at my x" as the dome overhead and snapped there instantly (the
            // B-under-the-hull teleport). Anchor on `prev_y` with the same INK_STEP_MAX continuity
            // bound as the walk arm; past it (or the ink decayed out) fall out as an aerial special.
            let p = paths[n.ground_ink as usize];
            n.pos.x += n.vel.x * DT;
            match ink_floor_y_near(&p, n.pos.x, prev_y, nodes) {
                Some(y) if (y - prev_y).abs() <= INK_STEP_MAX => {
                    n.pos.y = y;
                    n.vel.y = 0.0;
                }
                _ => {
                    n.ground_ink = -1;
                    n.ground_plat = -1;
                }
            }
        } else {
            // planted on a platform: pin y, slide x, fall out past the lip (physics.rs doc)
            crate::v1::physics::special_plat_pin(n, t);
        }
    } else if is_ledge(n.state) {
        // hanging / climbing: an ink lip re-pins to its (possibly moving) stroke each frame; a
        // stage lip stays fixed (set on grab, and at climb end).
        crate::v1::physics::ledge_ride(n, paths, nodes, t);
    // parity(v1-ink-grounded-follow): grounded fighters follow the same local curved face, inherit surface motion, slide downhill, drop through soft ink, and fall instead of snapping across discontinuities
    } else if n.on_ink() {
        // grounded on drawn ink: pin feet to the surface under us, walk off the ends, drop through
        // soft ink with held down (mirrors the soft-platform branch below). If the ink decayed out
        // from under us (no spanning segment), fall.
        let p = paths[n.ground_ink as usize];
        n.pos.x += n.vel.x * DT;
        // ride carry (`path_surface_vel`, step 4); ZERO vel is a no-op. Both axes now (row 4 dir.
        // B): `carried_y` feeds the gate + fall-y below; `step::repin_ink_riders` (dir. A) catches up.
        let ride = path_surface_vel(&p);
        n.pos.x += ride.x;
        let carried_y = prev_y + ride.y;
        // soft ink drops through via the same tilt-window buffer as a soft platform.
        let dropped = drop_through(n, i, t, !p.props.solid);
        // The pin follows the floor we're ALREADY STANDING ON, not the highest Floor anywhere above
        // the new x: inside a closed hull stroke (the ship) the dome overhead and the bowl floor
        // underneath both span the same x, and `ink_floor_y_at` always hands back the dome. Anchor
        // the lookup on `carried_y` (nearest-y-wins) so it tracks the standing face's curve; within
        // one walk step (`INK_STEP_MAX`) that's a smooth follow, past it there's no continuous
        // surface here — walked off an edge or over the cockpit opening — so fall instead of snapping.
        match ink_floor_y_near(&p, n.pos.x, carried_y, nodes) {
            Some(y) if !dropped && (y - carried_y).abs() <= INK_STEP_MAX => {
                n.pos.y = y;
                n.vel.y = 0.0;
                // Downslope slide: sample the surface just ahead/behind for its local tilt. Past
                // the stroke's floor_tol the fighter is pulled downhill (accel toward the lower
                // side); a flat floor (tilt ≤ floor_tol) adds nothing, so you stand. `sign(dy)` is
                // the downhill x-direction (y grows downward, so a surface that drops right slides
                // you right). Anchor both probes on `y` (the just-picked standing height) so they
                // stay on the same face instead of drifting onto a different span of the hull.
                let eps = 6.0;
                if let (Some(ya), Some(yb)) = (
                    ink_floor_y_near(&p, n.pos.x - eps, y, nodes),
                    ink_floor_y_near(&p, n.pos.x + eps, y, nodes),
                ) {
                    let dy = yb - ya;
                    if dy.abs() <= INK_STEP_MAX {
                        let tilt = dy.atan2(2.0 * eps).abs();
                        if tilt > p.props.floor_tol {
                            n.vel.x += dy.signum() * SLOPE_SLIDE_ACCEL * DT;
                        }
                    }
                }
            }
            _ => {
                // no floor under us, a discontinuity, or a drop-through: fall with normal gravity.
                // `carried_y`, not stale `prev_y`, so a fast-rising hull doesn't teleport it back.
                n.pos.y = carried_y;
                n.state = CharState::Air;
                n.ground_ink = -1;
                n.coyote = t.coyote_frames as u8;
            }
        }
    } else {
        // grounded: pinned to its platform, no vertical motion
        let p = PLATFORMS[n.ground_plat.clamp(0, PLATFORMS.len() as i32 - 1) as usize];
        n.pos.x += n.vel.x * DT;
        if drop_through(n, i, t, !p.solid) {
            n.state = CharState::Air;
            n.ground_plat = -1;
            n.coyote = t.coyote_frames as u8;
        } else {
            n.pos.y = p.y;
            n.vel.y = 0.0;
            // edges are sticky: only walk off when actively holding toward the edge, else
            // stop at the lip. Falling off no longer happens just from sliding momentum.
            if n.pos.x < p.left {
                if sgn < 0.0 {
                    n.state = CharState::Air;
                    n.ground_plat = -1;
                    n.coyote = t.coyote_frames as u8;
                    if p.solid {
                        n.regrab_lock = 12;
                    }
                } else {
                    n.pos.x = p.left;
                    n.vel.x = 0.0;
                }
            } else if n.pos.x > p.right {
                if sgn > 0.0 {
                    n.state = CharState::Air;
                    n.ground_plat = -1;
                    n.coyote = t.coyote_frames as u8;
                    if p.solid {
                        n.regrab_lock = 12;
                    }
                } else {
                    n.pos.x = p.right;
                    n.vel.x = 0.0;
                }
            }
        }
    }
    // snapshot for the on-real-floor invariant below: BEFORE the wall sweep's flush-clamp can nudge
    // x a hair off the x the ground branch above actually resolved y for.
    let pos_after_ground = n.pos;

    // walls: one swept pass over every Wall surf — the stage's side faces and drawn ink are the
    // same rows now. The leading ECB side vert pins flush; the wall's own bounce row (zero for
    // stage/platform walls) or the tumble kick, whichever is bigger, governs the reflect below.
    // Skipped while hanging a ledge. Touching a wall REFRESHES air resources (body-bus any-surf
    // rule): wall contact is a save, climbing your own ink is play.
    if !is_ledge(n.state) {
        // one-off getup teleport: `prev_pos` is a stale sweep origin this plant frame. Both
        // sweeps below now read the full ECB extent, not a sampled point (2026-07-07 fix) --
        // `sweep_walls` needs the same exclusion `apply_ink_containment` already had.
        let climbed = climbed_onto_stage_this_frame(prev_state, n.state);
        let wall = sweep_walls(prev_pos.x, n.pos, ECB_HALF_W, ECB_HALF_H, soup.surfs())
            .filter(|_| !climbed);
        if let Some(w) = wall {
            n.pos.x = w.x;
            if airborne(n.state) {
                // arm the walljump/cling window; hugging the wall keeps re-arming it each frame.
                // ONE arm seam shared with `apply_ink_containment` (body::arm_wall_cling).
                crate::v1::body::arm_wall_cling(n, w.nx, w.owner);
            }
            if n.vel.x * w.nx < 0.0 {
                // reflect (outward normal (nx,0)): TUMBLE bounces, NON-tumble DEAD-STOPS (drop-test).
                let e = if n.tumble {
                    w.restitution.max(t.wall_bounce)
                } else {
                    0.0
                };
                n.vel = geo::reflect(n.vel, Vector2::new(w.nx, 0.0), e);
            }
            touch_refresh(n, t);
        }

        // ink containment: box-vs-segment SAT resolver (body::apply_ink_containment's doc).
        if !climbed {
            apply_ink_containment(prev_pos, n, t, ECB_HALF_W, ECB_HALF_H, soup.surfs());
        }
    }

    // the auto-short-hop tag lives only for its one aerial; clear it the moment we touch down.
    if !airborne(n.state) {
        n.autohop_aerial = false;
    }

    // teleport-class guarantee (see body::assert_grounded_continuity / on_real_floor docs).
    let grounded_after = !airborne(n.state) && !is_ledge(n.state) && n.grounded();
    assert_grounded_continuity(
        grounded_before,
        grounded_after,
        n.pos.y - prev_y,
        INK_STEP_MAX,
    );
    debug_assert!(
        !grounded_after || on_real_floor(pos_after_ground, soup.surfs(), 1.5),
        "grounded fighter at {:?} has no real floor surf under it (drift/float bug)",
        n.pos
    );
    landing_frame
}

// ---- FSM-local helpers (relocated from lib: used only by reduce_next_state) ----

fn is_ledge(st: CharState) -> bool {
    matches!(st, CharState::LedgeHold | CharState::LedgeClimb)
}

/// True on the ONE frame `climb_onto_stage`'s position teleport just ran, not real continuous
/// motion: `LedgeHold` -> `LedgeRoll`/`LedgeAttack` (cur alone tells it, teleport ran before
/// `n.state` flipped), or `LedgeClimb` -> `Stand` (`cur == Stand` alone is too common; `prev ==
/// LedgeClimb` pins the one frame). Distinct from `is_ledge` on purpose: all three cur states
/// dispatch through ordinary movement arms afterward, so widening `is_ledge` would misroute their
/// actuation, not just skip a sweep (2026-07-06 ledge-roll regression, `apply_ink_containment`'s
/// doc; 2026-07-07 fix widened `sweep_walls` onto the same exclusion).
fn climbed_onto_stage_this_frame(prev: CharState, cur: CharState) -> bool {
    matches!(cur, CharState::LedgeRoll | CharState::LedgeAttack)
        || (prev == CharState::LedgeClimb && cur == CharState::Stand)
}

/// Smash charge: while the attack button stays down on the smash's FIRST frame, bank a charge
/// frame and report "pinned" (the caller force-resets, so the clock never leaves 0 — frame
/// data untouched). Release or the cap lets the swing out; `charge_mult` pays the bank out at
/// connect. A c-stick smash never pins (no button held), matching Smash.
fn charge_smash(n: &mut Fighter, i: &InputFrame, t: &Tune) -> bool {
    if n.frame == 0 && i.attack_held && n.charge < t.charge_max {
        n.charge += 1;
        true
    } else {
        false
    }
}

/// Feet up onto the grabbed ledge (ink lip -> onto that stroke; stage lip -> onto the main floor).
/// The body lives in `physics::ledge_climb` (za_warudo is at its R5 line cap); this is the
/// FSM-local name the getup / attack / roll / climb arms call.
fn climb_onto_stage(n: &mut Fighter, paths: &[InkPath; MAX_DRAWN], nodes: &[InkNode]) {
    crate::v1::physics::ledge_climb(n, paths, nodes);
}

/// Which foe (if any) is under this fighter's feet for a footstool: horizontally within the
/// victim's body radius, feet inside the band around their head crown. First match wins
/// (slot order — deterministic).
fn footstool_target(n: &Fighter, foes: &[Option<(Vector2, f32)>; MAX_PLAYERS]) -> Option<i8> {
    for (q, foe) in foes.iter().enumerate() {
        let Some((c, r)) = *foe else { continue };
        let head_y = c.y - r;
        if (n.pos.x - c.x).abs() <= r
            && n.pos.y >= head_y - FOOTSTOOL_ABOVE
            && n.pos.y <= head_y + FOOTSTOOL_BELOW
        {
            return Some(q as i8);
        }
    }
    None
}

/// Throw direction from the stick at the grab press: up / down / forward / back. `None` = a neutral
/// grab, which stays the soft toss (Act::Drop). aim_y is +down (matches the throw/DI convention).
/// Map a raw 2D flick to a throw direction by its dominant axis (screen y is down).
/// Jump / shield are available from every actionable ground state; factor them out.
/// Jump comes from the buffer so a slightly-early press still fires.
fn enter_grab(n: &mut Fighter) {
    n.clear_lane(Lane::Grab);
    n.arm_hits();
    n.grab_link = -1;
    n.vel.x *= 0.25; // same planting behavior for standing and jump-canceled grabs
    n.state = CharState::Grab;
}

fn try_ground_action(n: &mut Fighter, i: &InputFrame, atk: bool, grab: bool, t: &Tune) -> bool {
    if try_special(n) {
        return true;
    }
    if let Some(full) = take_jump(n) {
        n.state = CharState::JumpSquat;
        n.full_hop = full;
        true
    } else if grab {
        enter_grab(n);
        true
    } else if n.live(Lane::Strong) == Action::Strong {
        // c-stick on the ground: a smash in the flicked direction, aim captured at the flick.
        // Out of a dash/run a horizontal flick is the dash attack (PM), but an up flick still
        // reaches usmash directly — it slides, standing in for the jump-cancel from momentum.
        let ca = n.buf[Lane::Strong as usize].aim;
        n.clear_lane(Lane::Strong);
        n.arm_hits();
        let moving = matches!(n.state, CharState::Dash | CharState::Run);
        if moving && !(ca.y < 0.0 && ca.y.abs() > ca.x.abs()) {
            n.vel.x = n.facing * t.run_speed * 1.25;
            n.state = CharState::DashAttack;
        } else {
            enter_smash(n, ca, t);
        }
        true
    } else if atk || n.live(Lane::Attack) == Action::Attack {
        n.clear_lane(Lane::Attack);
        n.arm_hits();
        if n.flick_y_age as i64 <= t.smash_window && i.aim_y.abs() >= DASH_THRESH {
            // fresh vertical flick + attack = up/down smash (held up/down stays the tilt below).
            let dir = Vector2::new(0.0, sign(i.aim_y));
            enter_smash(n, dir, t);
        } else if n.flick_x_age as i64 <= t.smash_window && i.dir.abs() >= DASH_THRESH {
            // fresh horizontal flick + attack = forward smash toward the stick. Checked BEFORE the
            // dash/run arm: the flick that STARTS a dash is the same flick that authors an fsmash,
            // so for smash_window frames after it the smash wins (else every fsmash whose stick
            // landed a frame early would read as a dash attack).
            enter_smash(n, Vector2::new(sign(i.dir), 0.0), t);
        } else if matches!(n.state, CharState::Dash | CharState::Run) {
            // attacking out of momentum (flick gone stale) = a dash attack: drive a forward lunge
            // (faster than a plain run) so it carries even from a standing dash.
            n.vel.x = n.facing * t.run_speed * 1.25;
            n.state = CharState::DashAttack;
        } else if i.down {
            // down + attack from a standing/crouching pose = the down-tilt pothole.
            n.state = CharState::Dtilt;
            n.vel.x *= 0.25;
        } else if i.aim_y <= -0.35 {
            // held up + attack = the up-tilt anti-air.
            n.state = CharState::Utilt;
            n.vel.x *= 0.25;
        } else if i.dir.abs() >= WALK_THRESH {
            // held direction + attack = forward tilt; a back-held tilt turns you first.
            n.facing = sign(i.dir);
            n.state = CharState::Ftilt;
            n.vel.x *= 0.25;
        } else {
            n.state = CharState::Jab;
            n.vel.x *= 0.25; // plant feet: a standing jab mostly kills momentum on startup
        }
        true
    } else if i.shield_held {
        n.state = CharState::Shield;
        true
    } else {
        false
    }
}

/// Enter the smash state matching a flick/c-stick direction: vertical beats horizontal when the
/// aim is steeper. Fsmash faces the flick; usmash keeps its slide momentum (PM jump-cancel feel),
/// fsmash/dsmash plant in their state arms.
fn enter_smash(n: &mut Fighter, aim: Vector2, t: &Tune) {
    let _ = t;
    n.charge = 0; // fresh swing, fresh bank (the smash arms pin + fill it)
    let steep = aim.y.abs() > aim.x.abs();
    if steep && aim.y < 0.0 {
        n.state = CharState::Usmash;
    } else if steep && aim.y > 0.0 {
        n.state = CharState::Dsmash;
        n.vel.x *= 0.25;
    } else {
        if aim.x != 0.0 {
            n.facing = sign(aim.x);
        }
        n.state = CharState::Fsmash;
        n.vel.x *= 0.25;
    }
}

/// Soft-platform / soft-ink drop-through with a tilt-window buffer (Melee/PM feel). Crouch-holding
/// Down on a soft surface arms `drop_buf = plat_drop_window` and counts it down; while it counts the
/// fighter stays crouched, so a Down+Attack in the window converts to a Dtilt — that leaves Crouch
/// and cancels the pending drop. Releasing Down (Crouch -> Stand) or any non-Crouch state also
/// cancels it. Returns true on the frame the window expires still crouch-holding: that's the drop.
///
/// Keyed on `down` HELD, not the `down_pressed` edge: the shell only fires `down_pressed` for the
/// digital `ui_down` action, so a controller stick / touch stick sets `down` (via pad_down) but
/// never the edge. Reading the held bit makes the drop fire the same from every input source.
/// `soft` is false for the solid main stage / solid ink (never drops).
///
/// Tune `plat_drop_window`: 1 = drop on the crouch frame (near-instant, Melee-ish); larger = more
/// grace to convert into a Dtilt (PM-ish), at the cost of that many frames of drop latency.
fn drop_through(n: &mut Fighter, i: &InputFrame, t: &Tune, soft: bool) -> bool {
    // only a crouch-hold on a soft surface drops. Standing up, releasing Down, or converting to a
    // Dtilt (any non-Crouch state) clears the pending drop. Solid surfaces never drop.
    if !soft || n.state != CharState::Crouch || !i.down {
        n.drop_buf = 0;
        return false;
    }
    if n.drop_buf == 0 {
        n.drop_buf = t.plat_drop_window.max(1); // (re)arm the tilt-window on a fresh crouch-hold
    }
    n.drop_buf -= 1;
    n.drop_buf == 0 // window elapsed with no attack to convert it -> drop through
}

/// Consume a buffered jump/shorthop if one is live in the movement lane. Returns Some(full_hop).
fn take_jump(n: &mut Fighter) -> Option<bool> {
    match n.live(Lane::Movement) {
        Action::Jump => {
            n.clear_lane(Lane::Movement);
            Some(true)
        }
        Action::ShortHop => {
            n.clear_lane(Lane::Movement);
            Some(false)
        }
        _ => None,
    }
}
