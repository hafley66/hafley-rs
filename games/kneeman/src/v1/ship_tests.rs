// The parked ship (plans/lovers-ship.md): hull = baked circle stroke, helm = rider c-stick,
// booster exhaust = knockback volume off the rim. Declared as a crate-root child so
// `super::*` is the crate root.

use super::*;

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

fn tune() -> Tune {
    Tune::from_char(&CharData::KNEEMAN)
}

/// Fighter 0 dropped onto the dome and settled; fighter 1 parked far away on the main stage.
fn crewed() -> (SimState, Tune) {
    let t = tune();
    let mut s = SimState::spawn();
    s.fighters[0].pos = Vector2::new(SHIP_HOME.x, SHIP_HOME.y - SHIP_R - 40.0);
    s.fighters[0].vel = Vector2::ZERO;
    s.fighters[0].state = CharState::Air;
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y);
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_plat = 0;
    for _ in 0..60 {
        s = step(&s, &[&IDLE, &IDLE], &t);
    }
    (s, t)
}

/// Fighter 0 seated at the ship's one station (the helm fixture `SimState::spawn` seeds, occupied
/// via a GRAB press -- plans/ship-containment.md #3: attack alone no longer occupies, so an
/// ordinary jab thrown after landing doesn't hijack into piloting); fighter 1 parked on the main
/// stage. The occupy press itself never leaks into a steer/thrust read: `station` is still -1
/// when `ship::steer_from_riders` runs at the top of THIS same step (steer runs before the FSM
/// phase that actuates `Act::Occupy`), so the seat only starts piloting next frame.
fn seated() -> (SimState, Tune) {
    let (mut s, t) = crewed();
    let grab = InputFrame { grab: true, ..IDLE };
    s = step(&s, &[&grab, &IDLE], &t);
    assert_eq!(
        s.fighters[0].station, 0,
        "seated for the steer/thrust tests"
    );
    (s, t)
}

/// The item slot of the helm station fixture `SimState::spawn` seeds (mount 0).
fn station_slot(s: &SimState) -> usize {
    s.items
        .iter()
        .position(|it| it.active() && it.mount == 0)
        .expect("spawn seeds the helm station fixture")
}

#[test]
fn hull_is_enterable_terrain() {
    // dropped over the center: falls THROUGH the top hatch and lands inside the bowl.
    let (s, _t) = crewed();
    let f = &s.fighters[0];
    assert_eq!(
        f.ground_ink, SHIP_SLOT as i8,
        "settled on the hull stroke, state={:?}",
        f.state
    );
    let bowl_floor = SHIP_HOME.y + SHIP_R;
    assert!(
        (f.pos.y - bowl_floor).abs() < 20.0,
        "feet inside the cockpit bowl, y={}",
        f.pos.y
    );
}

#[test]
fn groundling_on_hull_smashes_not_steers() {
    // v3 simplification (plans/lovers-ship.md): the gate is OCCUPYING the seat, not standing on
    // the hull. A fighter merely settled on the hull surface (never interacted with the station)
    // keeps its Strong lane and never pilots the ship.
    let (mut s, t) = crewed();
    let flick = InputFrame { cx: 1.0, ..IDLE };
    s = step(&s, &[&flick, &IDLE], &t);
    assert_eq!(
        s.helm.thrust, 0.0,
        "standing on the hull alone does not steer"
    );
    let mut smashed = false;
    for _ in 0..20 {
        s = step(&s, &[&IDLE, &IDLE], &t);
        smashed |= s.fighters[0].state == CharState::Fsmash;
    }
    assert!(
        smashed,
        "off the seat, standing on the hull still buys a smash"
    );
}

#[test]
fn seated_cstick_aims_without_thrust() {
    // Seated: the c-stick is AIM ONLY now (sticky -- survives an idle frame). No thrust from the
    // stick alone; that is attack's job (see `seated_attack_thrusts`).
    let (mut s, t) = seated();
    let steer = InputFrame { cx: 1.0, ..IDLE };
    s = step(&s, &[&steer, &IDLE], &t);
    assert!(
        (s.helm.aim - Vector2::new(1.0, 0.0)).length() < 1e-4,
        "c-stick sets the aim"
    );
    assert_eq!(s.helm.thrust, 0.0, "c-stick alone no longer thrusts");
    s = step(&s, &[&IDLE, &IDLE], &t);
    assert!(
        (s.helm.aim - Vector2::new(1.0, 0.0)).length() < 1e-4,
        "aim is sticky: survives the idle frame"
    );
    assert_eq!(s.helm.thrust, 0.0);
}

#[test]
fn seated_attack_thrusts() {
    // Attack = the boost while seated. Aim is set by an earlier c-stick flick and held; the
    // attack press/hold is what drives thrust to 1.0, and thrust drops the instant it's released
    // (it does not survive an idle frame the way aim does).
    let (mut s, t) = seated();
    let aim_in = InputFrame { cx: 1.0, ..IDLE };
    s = step(&s, &[&aim_in, &IDLE], &t);
    let boost = InputFrame {
        attack: true,
        attack_held: true,
        ..IDLE
    };
    s = step(&s, &[&boost, &IDLE], &t);
    assert_eq!(s.helm.thrust, 1.0, "attack while seated is the boost");
    assert!(
        (s.helm.aim - Vector2::new(1.0, 0.0)).length() < 1e-4,
        "aim held from the earlier flick"
    );
    s = step(&s, &[&IDLE, &IDLE], &t);
    assert_eq!(s.helm.thrust, 0.0, "thrust dies immediately on release");
}

#[test]
fn groundling_cstick_never_steers() {
    let (mut s, t) = crewed();
    let flick = InputFrame { cx: 1.0, ..IDLE };
    // fighter 1 stands on the main stage: its c-stick is still a smash macro
    s = step(&s, &[&IDLE, &flick], &t);
    assert_eq!(s.helm.thrust, 0.0, "only hull riders drive the engine");
    let mut smashed = false;
    for _ in 0..20 {
        s = step(&s, &[&IDLE, &IDLE], &t);
        smashed |= s.fighters[1].state == CharState::Fsmash;
    }
    assert!(smashed, "off-ship the flick still buys a smash");
}

#[test]
fn booster_blasts_down_the_exhaust() {
    let (mut s, t) = seated();
    // aim RIGHT first (flame out the back, left) -- a separate frame from the boost so the
    // victim's position below isn't perturbed by an extra gravity frame first.
    let aim_in = InputFrame { cx: 1.0, ..IDLE };
    s = step(&s, &[&aim_in, &IDLE], &t);
    // victim hovers at the LEFT rim, outside the hull — dead center of the exhaust sector.
    s.fighters[1].pos = Vector2::new(SHIP_HOME.x - SHIP_R - 60.0, SHIP_HOME.y);
    s.fighters[1].vel = Vector2::ZERO;
    s.fighters[1].state = CharState::Air;
    s.fighters[1].ground_plat = -1;
    let boost = InputFrame {
        attack: true,
        attack_held: true,
        ..IDLE
    };
    s = step(&s, &[&boost, &IDLE], &t);
    let v = &s.fighters[1];
    assert_eq!(v.state, CharState::Launched, "exhaust launches");
    assert_eq!(v.hitstun, t.booster_stun);
    assert!(
        v.vel.x < -t.booster_kb * 0.9,
        "blown out the back (left), vel.x={}",
        v.vel.x
    );
}

#[test]
fn idle_engine_does_not_blast() {
    // Seated but not pressing attack: no thrust, no blast.
    let (mut s, t) = seated();
    s.fighters[1].pos = Vector2::new(SHIP_HOME.x - SHIP_R - 60.0, SHIP_HOME.y);
    s.fighters[1].vel = Vector2::ZERO;
    s.fighters[1].state = CharState::Air;
    s.fighters[1].ground_plat = -1;
    let s = step(&s, &[&IDLE, &IDLE], &t);
    let v = &s.fighters[1];
    assert_ne!(v.state, CharState::Launched, "no throttle, no flame");
    assert_eq!(v.hitstun, 0);
}

// ── entry force-field (plans/ship-containment.md #2/#3, 2026-07-06 playtest) ───────────────────────
//
// "entering the ball has weird force field, maybe the pilot item or a ledge initiate." Repro'd all
// four candidates: the container-lip ledge-snap was already fixed by the directional grab zone
// (commit 1/5 -- an entering body never grabs the hatch lip, see hull_gate_tests); a plain fall-in
// with IDLE input only ever changes velocity via the intended solid-hull bounce (commit 2/5), never
// an unexplained snap. Two REAL culprits: (1) an ordinary ATTACK press near the helm (whose anchor
// sits exactly where a crew member naturally lands after dropping through the hatch) used to hijack
// into `Act::Occupy`, hard-zeroing velocity and freezing the FSM every frame after -- fixed by
// requiring GRAB (station.rs::occupy_intent); (2) `booster_blast` could launch a body already
// CONTAINED by the hull (grounded on it) with zero input of its own, whenever another rider thrust
// -- fixed by excluding `ground_ink == SHIP_SLOT` and the seated pilot from the blast (ship.rs).

#[test]
fn entering_with_momentum_preserves_velocity_until_a_helm_mount_input_fires() {
    // A body falls into the hull with horizontal momentum and IDLE input the whole time: it must
    // retain its momentum (bouncing off the solid walls per commit 2/5, never a force-field snap)
    // and never gets teleported/zeroed/stationed absent a deliberate grab-the-helm press.
    let t = tune();
    let mut s = SimState::spawn();
    s.fighters[0].pos = Vector2::new(SHIP_HOME.x - 40.0, SHIP_HOME.y - SHIP_R - 40.0);
    s.fighters[0].vel = Vector2::new(220.0, 100.0);
    s.fighters[0].state = CharState::Air;
    s.fighters[0].ground_ink = -1;
    s.fighters[0].ground_plat = -1;
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y);
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_plat = 0;
    let mut ever_moving = false;
    for frame in 0..90 {
        s = step(&s, &[&IDLE, &IDLE], &t);
        let f = &s.fighters[0];
        assert_eq!(
            f.station, -1,
            "no button ever pressed: never auto-occupies (frame {frame})"
        );
        if f.vel.length() > 1.0 {
            ever_moving = true;
        }
    }
    assert!(
        ever_moving,
        "momentum must carry/bounce through the fall, never dead-frozen the instant it enters"
    );
}

#[test]
fn attack_near_the_station_never_occupies_it() {
    // The fix: attack alone is an ordinary jab everywhere, including standing right on the helm's
    // anchor -- only a deliberate GRAB press mounts it.
    let (mut s, t) = crewed();
    let attack = InputFrame {
        attack: true,
        ..IDLE
    };
    s = step(&s, &[&attack, &IDLE], &t);
    assert_eq!(
        s.fighters[0].station, -1,
        "attack must never occupy the station"
    );
    assert!(
        !airborne(s.fighters[0].state) && s.fighters[0].state != CharState::Stand,
        "an ordinary attack near the helm still starts an ordinary jab, state={:?}",
        s.fighters[0].state
    );
}

#[test]
fn contained_crew_exempt_from_the_ships_own_exhaust() {
    // A crew member already grounded ON the hull, near the rim, must not get launched by their own
    // ship's exhaust just for standing there while someone else pilots + thrusts.
    let t = tune();
    let x = SHIP_HOME.x - SHIP_R + 20.0; // near the rim, close to the exhaust annulus
    let y = ink_floor_y_at(
        &SimState::spawn().paths[SHIP_SLOT],
        x,
        &SimState::spawn().nodes,
    )
    .expect("valid hull x");
    let mut s = SimState::spawn();
    s.fighters[0].pos = Vector2::new(x, y);
    s.fighters[0].vel = Vector2::ZERO;
    s.fighters[0].state = CharState::Stand;
    s.fighters[0].ground_ink = SHIP_SLOT as i8;
    s.fighters[0].ground_plat = 0;
    s.fighters[1].pos = station_anchor(&s.paths, 0).unwrap();
    s.fighters[1].station = 0;
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_plat = 0;
    s.fighters[1].ground_ink = SHIP_SLOT as i8;
    let thrust = InputFrame {
        cx: 1.0,
        attack_held: true,
        ..IDLE
    };
    for frame in 0..10 {
        s = step(&s, &[&IDLE, &thrust], &t);
        let f = &s.fighters[0];
        assert_ne!(
            f.state,
            CharState::Launched,
            "contained crew must never eat their own ship's exhaust (frame {frame})"
        );
    }
}

/// Grounded fighter 0 on the hull surface at world-x `x`, walking with `facing`; fighter 1 parked
/// far away on the main stage. No settling loop — the caller drives the walk.
fn grounded_on_hull(x: f32, facing: f32) -> (SimState, Tune) {
    let t = tune();
    let mut s = SimState::spawn();
    let y = ink_floor_y_at(&s.paths[SHIP_SLOT], x, &s.nodes)
        .expect("x is over a walkable hull segment");
    s.fighters[0].pos = Vector2::new(x, y);
    s.fighters[0].vel = Vector2::ZERO;
    s.fighters[0].state = CharState::Stand;
    s.fighters[0].facing = facing;
    s.fighters[0].ground_ink = SHIP_SLOT as i8;
    s.fighters[0].ground_plat = 0;
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y);
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_plat = 0;
    (s, t)
}

#[test]
fn walking_over_the_rim_falls_in_without_teleporting() {
    // The playtest bug (plans/ac-ship-backlog.md item 2): walking off the dome over the cockpit
    // opening used to TELEPORT the fighter down into the bowl, then across to the far ledge —
    // the grounded-ink pin snapped to the nearest Floor even when it was a whole hull-diameter
    // away. Fixed: a discontinuous drop drops you into Air (fall in with gravity) instead.
    let start_x = SHIP_HOME.x - 0.45 * SHIP_R; // on the upper-left dome shoulder, left of the hatch
    let (mut s, t) = grounded_on_hull(start_x, 1.0);
    let walk = InputFrame { dir: 1.0, ..IDLE };
    let mut prev = s.fighters[0].pos;
    let mut entered = false;
    for _ in 0..240 {
        s = step(&s, &[&walk, &IDLE], &t);
        let f = &s.fighters[0];
        // Stop once the body has left the hull outward through the one-way purple equator
        // (2026-07-05: the rim is permeable outward, so an angled entry can pass on through). The
        // no-teleport guard below only holds WHILE the body is still in the hull's vicinity -- once
        // it clears the rim and heads for the stage/blast zone, its arc is ordinary flight.
        if (f.pos - SHIP_HOME).length() > SHIP_R + 40.0 {
            break;
        }
        let disp = (f.pos - prev).length();
        assert!(
            disp < 90.0,
            "per-frame displacement bounded (no teleport): {disp}px at {:?}",
            f.pos
        );
        prev = f.pos;
        // fell in through the hatch and grounded on the interior bowl floor
        if !airborne(f.state) && f.ground_ink == SHIP_SLOT as i8 && f.pos.y > SHIP_HOME.y {
            entered = true;
            break;
        }
    }
    // walking off the dome over the hatch drops the fighter through the opening (no teleport across
    // the hull): it either grounds inside the bowl or arcs out the far one-way rim -- both are the
    // fall-in-with-gravity fix, never the old snap-to-the-far-ledge teleport.
    let f = &s.fighters[0];
    assert!(
        entered || (f.pos - SHIP_HOME).length() > SHIP_R,
        "fell through the opening (grounded inside or arced out), pos={:?} state={:?}",
        f.pos,
        f.state
    );
}

#[test]
fn dropped_above_the_opening_falls_in_with_gravity() {
    // Drop straight down the hatch: a plain gravity fall that lands on the bowl floor, never a
    // sideways or upward snap on the way in.
    let t = tune();
    let mut s = SimState::spawn();
    s.fighters[0].pos = Vector2::new(SHIP_HOME.x, SHIP_HOME.y - SHIP_R - 30.0);
    s.fighters[0].vel = Vector2::ZERO;
    s.fighters[0].state = CharState::Air;
    s.fighters[0].ground_ink = -1;
    s.fighters[0].ground_plat = -1;
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y);
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_plat = 0;

    let mut prev_y = s.fighters[0].pos.y;
    let mut landed = false;
    for _ in 0..180 {
        s = step(&s, &[&IDLE, &IDLE], &t);
        let f = &s.fighters[0];
        if airborne(f.state) {
            assert!(
                f.pos.y >= prev_y - 1.0,
                "monotonic descent (no upward snap): y={} prev={}",
                f.pos.y,
                prev_y
            );
            assert!(
                (f.pos.x - SHIP_HOME.x).abs() < 40.0,
                "falls straight down the hatch (no lateral snap): x={}",
                f.pos.x
            );
            prev_y = f.pos.y;
        } else {
            landed = true;
            assert_eq!(f.ground_ink, SHIP_SLOT as i8, "landed on the hull stroke");
            assert!(
                f.pos.y > SHIP_HOME.y,
                "settled in the lower bowl, y={}",
                f.pos.y
            );
            break;
        }
    }
    assert!(landed, "settled onto the hull floor within 180 frames");
}

#[test]
fn walking_the_bowl_floor_tracks_the_hull_not_the_air() {
    // The follow-up playtest bug (Chris, verbatim): "walking inside bottom of ship allows u to
    // walk on air and leave the circle from the bottom." Root cause: `ink_floor_y_at` always
    // returns the HIGHEST Floor spanning x — standing in the bowl, the dome overhead wins every
    // frame, so the old rise-hold guard froze pos.y at the bowl-bottom height while pos.x kept
    // advancing, carrying the fighter off the hull's own curve. Fixed: the pin now follows the
    // Floor nearest the previous y, so every grounded frame stays ON the hull circle.
    let start_x = SHIP_HOME.x; // dead center of the bowl floor
    let (mut s, t) = grounded_on_hull(start_x, 1.0);
    let walk = InputFrame { dir: 1.0, ..IDLE };
    let mut prev = s.fighters[0].pos;
    let mut saw_grounded = false;
    for _ in 0..300 {
        s = step(&s, &[&walk, &IDLE], &t);
        let f = &s.fighters[0];
        // stop once the walk has carried the fighter OUT through the one-way purple equator
        // (2026-07-05): past the rim its motion is ordinary flight (then a respawn teleport), which
        // this grounded-pin no-teleport guard is not about.
        if (f.pos - SHIP_HOME).length() > SHIP_R + 40.0 {
            break;
        }
        let disp = (f.pos - prev).length();
        assert!(
            disp < 90.0,
            "per-frame displacement bounded: {disp}px at {:?}",
            f.pos
        );
        prev = f.pos;
        if !airborne(f.state) && f.ground_ink == SHIP_SLOT as i8 {
            saw_grounded = true;
            let d = (f.pos - SHIP_HOME).length();
            assert!(
                d <= SHIP_R + 4.0,
                "grounded on the hull must stay INSIDE/ON the circle, never past it: d={d} pos={:?}",
                f.pos
            );
            assert!(
                d >= SHIP_R - 20.0,
                "grounded on the hull must stay NEAR its surface, not float toward the center: \
                 d={d} pos={:?}",
                f.pos
            );
        }
    }
    assert!(
        saw_grounded,
        "fighter must spend at least some frames grounded on the hull"
    );
}

#[test]
fn walking_up_the_bowl_follows_the_curve_then_stalls_at_the_solid_equator() {
    // Walking from the bowl bottom toward a side: y must trace the hull's own curve (climbing as
    // |x - center| grows), never hold a "highest Floor" height plucked from clear across the hull.
    // 2026-07-06 (plans/ship-containment.md #1, SUPERSEDES the 97d11f5 one-way-purple decision):
    // the purple equator is SOLID both ways again -- a solid bouncy container, exit only via the
    // hatch gap -- so walking into the steep face stalls (never exits), same as the blue shoulder
    // above it. The curve-tracking guard still holds every grounded frame.
    let start_x = SHIP_HOME.x + 10.0; // just off dead-center, walking rightward up the curve
    let (mut s, t) = grounded_on_hull(start_x, 1.0);
    let walk = InputFrame { dir: 1.0, ..IDLE };
    let start_y = s.fighters[0].pos.y;
    let mut max_x_while_grounded = start_x;
    let mut y_at_max_x = start_y;
    let mut grounded_frames = 0u32;
    for _ in 0..400 {
        s = step(&s, &[&walk, &IDLE], &t);
        let f = &s.fighters[0];
        if !airborne(f.state) && f.ground_ink == SHIP_SLOT as i8 {
            grounded_frames += 1;
            // while grounded on the hull the pin stays ON the circle (never floats to center / past
            // the rim), the tracking invariant this test guards.
            let d = (f.pos - SHIP_HOME).length();
            assert!(
                d <= SHIP_R + 6.0,
                "grounded on the hull must ride the circle, not float past it: d={d} pos={:?}",
                f.pos
            );
            if f.pos.x > max_x_while_grounded {
                max_x_while_grounded = f.pos.x;
                y_at_max_x = f.pos.y;
            }
        }
        assert!(
            (f.pos - SHIP_HOME).length() <= SHIP_R + 40.0,
            "the solid equator must never let a walk exit the hull, pos={:?}",
            f.pos
        );
    }
    assert!(
        grounded_frames > 5,
        "must have walked the bowl curve grounded across several frames, got {grounded_frames}"
    );
    // climbed a real distance up the curve, y rising to meet it (screen-y shrinking) -- not
    // stuck at the bowl-bottom height while x alone advanced (the original tracking bug).
    assert!(
        max_x_while_grounded - start_x > 100.0,
        "must climb meaningfully away from center: reached x={max_x_while_grounded}"
    );
    assert!(
        start_y - y_at_max_x > 80.0,
        "y must climb (shrink) to match the curve, not stay flat: start_y={start_y} \
         y_at_max_x={y_at_max_x}"
    );
}

#[test]
fn ac_boost_climbs_out_of_the_bowl() {
    // The bigger hull is DEEP: full-hop + double-jump reach ~504px but the rim sits ~597px above
    // the bowl floor, so base jumps alone do NOT clear it (flagged in the report). The Armored
    // Core boost is the way out — sustained upward thrust that beats the doubled gravity.
    let (mut s, t) = crewed(); // fighter 0 settled on the hull floor inside the bowl
    assert_eq!(
        s.fighters[0].ground_ink, SHIP_SLOT as i8,
        "starts crewed inside the bowl"
    );
    let rim_y = SHIP_HOME.y - 0.951 * SHIP_R; // cockpit lip height (vertices 14/16)
    s.fighters[0].badges |= Badge::AcCore as u8;
    let mut escaped = false;
    for f in 0..240 {
        // jump off the floor once, then hold jump with the stick up = constant boost out the hatch
        let inp = if f == 0 {
            InputFrame {
                jump: true,
                jump_held: true,
                aim_y: -1.0,
                ..IDLE
            }
        } else {
            InputFrame {
                jump_held: true,
                aim_y: -1.0,
                ..IDLE
            }
        };
        s = step(&s, &[&inp, &IDLE], &t);
        if s.fighters[0].pos.y < rim_y {
            escaped = true;
            break;
        }
    }
    assert!(
        escaped,
        "AC boost clears the cockpit rim (feet above y={rim_y}), ended at y={}",
        s.fighters[0].pos.y
    );
}

// ── station mount v1 (plans/lovers-ship.md "v2: stations") ───────────────────────────────────────

#[test]
fn occupying_locks_pos_to_the_anchor() {
    // A crew member interacts with a mounted station and is LOCKED to the anchor: pos snaps there
    // and stays pinned frame after frame, and the station item is NOT pocketed.
    let (mut s, t) = crewed();
    let slot = station_slot(&s);
    assert_eq!(s.items[slot].mount, 0, "seeded item is mounted to anchor 0");
    let anchor = station_anchor(&s.paths, 0).unwrap();

    // interact (grab) over the station in reach: occupies instead of pocketing / jabbing. Attack
    // alone must NOT occupy (plans/ship-containment.md #3) -- it stays free for ordinary combat.
    let grab = InputFrame { grab: true, ..IDLE };
    s = step(&s, &[&grab, &IDLE], &t);
    assert_eq!(
        s.fighters[0].station, 0,
        "grab over the station occupied it"
    );
    assert_eq!(
        s.fighters[0].holding, -1,
        "occupying does not pocket the station"
    );
    assert!(
        s.items[slot].active() && s.items[slot].mount == 0,
        "the station item stays mounted after occupy"
    );

    // pos is pinned to the anchor across every following frame (the Grabbed-style follow)
    for f in 0..40 {
        s = step(&s, &[&IDLE, &IDLE], &t);
        assert_eq!(
            s.fighters[0].station, 0,
            "stays occupied on idle (frame {f})"
        );
        assert!(
            (s.fighters[0].pos - anchor).length() < 1e-3,
            "pos pinned to the anchor at frame {f}: {:?} vs anchor {:?}",
            s.fighters[0].pos,
            anchor
        );
    }
}

#[test]
fn jump_releases_the_station_and_restores_movement() {
    // Jump breaks the lock: the rider pops out of the seat, becomes airborne, and is free to move
    // again (leaves the anchor point) instead of staying pinned.
    let (mut s, t) = crewed();
    let grab = InputFrame { grab: true, ..IDLE };
    s = step(&s, &[&grab, &IDLE], &t);
    assert_eq!(s.fighters[0].station, 0, "occupied");
    // settle the lock a couple frames
    s = step(&s, &[&IDLE, &IDLE], &t);
    s = step(&s, &[&IDLE, &IDLE], &t);
    let seat = s.fighters[0].pos;

    let jump = InputFrame {
        jump: true,
        jump_held: true,
        ..IDLE
    };
    s = step(&s, &[&jump, &IDLE], &t);
    assert_eq!(s.fighters[0].station, -1, "jump released the station");
    assert!(
        airborne(s.fighters[0].state),
        "released into the air, state={:?}",
        s.fighters[0].state
    );
    assert!(s.fighters[0].vel.y < 0.0, "popped upward out of the seat");

    // free again: it leaves the anchor point under normal physics
    let mut moved = false;
    for _ in 0..90 {
        s = step(&s, &[&IDLE, &IDLE], &t);
        assert_eq!(s.fighters[0].station, -1, "stays released");
        if (s.fighters[0].pos - seat).length() > 20.0 {
            moved = true;
            break;
        }
    }
    assert!(moved, "normal movement restored after release");
}

#[test]
fn mounted_station_never_falls_or_despawns_but_a_free_item_does() {
    // The mount fixture is exempt from the item physics/decay pass: it never moves and never clears,
    // while an ordinary unowned item dropped where no floor catches it falls and despawns.
    let mut t = tune();
    t.items_on = false; // no stray random spawns cluttering the field mid-test
    let mut s = SimState::spawn(); // hull + helm station are baked from frame 0
    let st_slot = station_slot(&s);
    let anchor = station_anchor(&s.paths, 0).unwrap();

    // a free (unmounted) item off-world, left of the hull where no platform/hull catches it
    let free = s.items.iter().position(|it| !it.active()).unwrap();
    s.items[free] = Item {
        kind: ItemKind::LaserGun,
        pos: Vector2::new(-540.0, 100.0),
        gas: 16.0,
        gas_max: 16.0,
        ..Item::EMPTY
    };
    assert_eq!(
        s.items[free].mount, -1,
        "the comparison item is a free item"
    );

    let mut free_fell = false;
    for _ in 0..600 {
        let before_free_y = s.items[free].pos.y;
        let free_was_active = s.items[free].active();
        s = step(&s, &[&IDLE, &IDLE], &t);
        // mounted station: pinned to the anchor, never cleared
        assert!(
            s.items[st_slot].active() && s.items[st_slot].mount == 0,
            "mounted station stays mounted"
        );
        assert!(
            (s.items[st_slot].pos - anchor).length() < 1e-3,
            "mounted station never moves: {:?} vs {:?}",
            s.items[st_slot].pos,
            anchor
        );
        if free_was_active && s.items[free].active() && s.items[free].pos.y > before_free_y {
            free_fell = true;
        }
        if !s.items[free].active() {
            break;
        }
    }
    assert!(free_fell, "the free item fell under gravity");
    assert!(
        !s.items[free].active(),
        "the free item despawned past the blast zone"
    );
    assert!(
        s.items[st_slot].active(),
        "the mounted station is still here after the free item is long gone"
    );
}

#[test]
fn mounted_station_item_rides_the_flying_hull() {
    // 2026-07-04 playtest: "the item for the control helm does not move" -- the console's pos
    // was written once at spawn, so the flying ship left it parked behind (and out of occupy
    // reach, which reads the ITEM's pos). update_items re-pins mounted items to the live
    // anchor every frame, the same `station_anchor` read the seated pilot follows.
    let (mut s, t) = seated();
    let slot = station_slot(&s);
    let start = s.items[slot].pos;
    let boost = InputFrame {
        cx: 1.0,
        attack: true,
        attack_held: true,
        ..IDLE
    };
    for f in 0..40 {
        s = step(&s, &[&boost, &IDLE], &t);
        let anchor = station_anchor(&s.paths, 0).unwrap();
        assert!(
            (s.items[slot].pos - anchor).length() < 1e-3,
            "console pinned to the live anchor at frame {f}: {:?} vs {:?}",
            s.items[slot].pos,
            anchor
        );
    }
    assert!(
        (s.items[slot].pos - start).length() > 5.0,
        "the console actually flew with the ship: {:?} -> {:?}",
        start,
        s.items[slot].pos
    );
}

#[test]
fn a_second_fighter_cannot_take_an_occupied_station() {
    // One rider per anchor: with fighter 0 locked in, fighter 0 keeps it and a second interacting
    // fighter is refused (its `station` stays -1).
    let (mut s, t) = crewed();
    let anchor = station_anchor(&s.paths, 0).unwrap();

    let grab = InputFrame { grab: true, ..IDLE };
    s = step(&s, &[&grab, &IDLE], &t);
    assert_eq!(s.fighters[0].station, 0, "fighter 0 occupied first");

    // drop fighter 1 right onto the station, grounded on the hull, and have it interact
    s.fighters[1].pos = anchor;
    s.fighters[1].vel = Vector2::ZERO;
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_ink = SHIP_SLOT as i8;
    s.fighters[1].ground_plat = 0;
    s.fighters[1].holding = -1;

    for f in 0..10 {
        s = step(&s, &[&IDLE, &grab], &t);
        assert_eq!(
            s.fighters[1].station, -1,
            "station is taken; fighter 1 can't occupy it (frame {f})"
        );
        assert_eq!(s.fighters[0].station, 0, "fighter 0 keeps the station");
    }
}

// ── unpark the ship (plans/body-unify.md step 6) ──────────────────────────────────────────────────

#[test]
fn uncrewed_ship_stays_parked_with_mass() {
    // The hard constraint: giving the hull mass (density > 0) must not, by itself, move it. With
    // no seated pilot ever firing thrust and `gravity_scale` zeroed, the hull is a real body that
    // simply never receives a force -- parked behaves exactly as before.
    let t = tune();
    let mut s = SimState::spawn();
    assert!(
        s.paths[SHIP_SLOT].mass > 0.0,
        "the hull is a real body now (step 6)"
    );
    for _ in 0..180 {
        s = step(&s, &[&IDLE, &IDLE], &t);
    }
    assert_eq!(
        s.paths[SHIP_SLOT].pos, SHIP_HOME,
        "parked hull never drifts without thrust"
    );
    assert_eq!(
        s.paths[SHIP_SLOT].vel,
        Vector2::ZERO,
        "parked hull never picks up velocity"
    );
}

#[test]
fn seated_thrust_accelerates_the_hull_along_aim() {
    // The exhaust's reaction: `helm.thrust` (attack, held, while seated) pushes the hull's own
    // vel along `helm.aim` every frame it burns -- an acceleration row (`Tune::ship_thrust_accel`),
    // never a hover/counter-gravity term (the hull's `gravity_scale` row is 0, so this is the only
    // force that ever touches it).
    let (mut s, t) = seated();
    assert_eq!(s.paths[SHIP_SLOT].vel, Vector2::ZERO, "not yet thrusting");
    let boost = InputFrame {
        cx: 1.0,
        attack: true,
        attack_held: true,
        ..IDLE
    };
    s = step(&s, &[&boost, &IDLE], &t);
    assert_eq!(
        s.helm.aim,
        Vector2::new(1.0, 0.0),
        "this frame's flick set the aim"
    );
    assert_eq!(s.helm.thrust, 1.0, "attack seated is the boost");
    let expected_step = t.ship_thrust_accel * DT * DT; // one frame's accel, native ink units
    assert!(
        (s.paths[SHIP_SLOT].vel - Vector2::new(expected_step, 0.0)).length() < 1e-4,
        "hull.vel after one thrust frame: {:?}, expected ({expected_step}, 0)",
        s.paths[SHIP_SLOT].vel
    );
    // hold the boost: vel keeps growing along aim, never perpendicular to it.
    for _ in 0..30 {
        s = step(&s, &[&boost, &IDLE], &t);
    }
    let v = s.paths[SHIP_SLOT].vel;
    assert!(
        v.x > expected_step * 10.0,
        "sustained thrust keeps accelerating the hull: {v:?}"
    );
    assert!(
        v.y.abs() < 1e-3,
        "aim was pure +x: no perpendicular component, got {v:?}"
    );
    assert!(
        s.paths[SHIP_SLOT].pos.x > SHIP_HOME.x,
        "the hull actually moved: pos={:?}",
        s.paths[SHIP_SLOT].pos
    );
}

#[test]
fn seated_rider_tracks_the_flying_hull() {
    // The station lock (existing machinery, unedited by step 6): `stationed_step` pins the
    // seated pilot to `station_anchor` -- but reads it from the FSM-phase snapshot, taken before
    // this same frame's thrust/`integrate_ink` move the hull, so the pin is exactly one frame
    // behind the hull's own (still-accelerating) velocity -- never a growing/unbounded drift,
    // always bounded by the CURRENT native-frame vel, same documented shape as the standing-crew
    // caveat below. `< vel.length() + slop` pins that bound instead of an arbitrary tolerance.
    let (mut s, t) = seated();
    let boost = InputFrame {
        cx: 1.0,
        attack: true,
        attack_held: true,
        ..IDLE
    };
    let start_anchor = station_anchor(&s.paths, 0).unwrap();
    for f in 0..40 {
        s = step(&s, &[&boost, &IDLE], &t);
        let anchor = station_anchor(&s.paths, 0).unwrap();
        let bound = s.paths[SHIP_SLOT].vel.length() + 0.25;
        assert!(
            (s.fighters[0].pos - anchor).length() <= bound,
            "seated pilot within one frame's hull-vel of the anchor at frame {f}: {:?} vs {:?} (bound {bound})",
            s.fighters[0].pos,
            anchor
        );
    }
    let end_anchor = station_anchor(&s.paths, 0).unwrap();
    assert!(
        (end_anchor - start_anchor).length() > 5.0,
        "the anchor (and the seated pilot with it) actually flew somewhere: {:?} -> {:?}",
        start_anchor,
        end_anchor
    );
}

#[test]
fn standing_crew_rides_the_flying_hull_horizontally() {
    // Step 4's ride-carry seam (`path_surface_vel`) is dormant-turned-live for the hull now that
    // thrust actually moves it: a fighter grounded on the hull surface but NOT seated (ordinary
    // Stand, no station) must track its horizontal motion, same as the mover/drifting-stroke
    // tests, but with the ACTUAL flying hull as the surface. Pure +x aim keeps the hull's vel.y at
    // 0 throughout (gravity_scale is 0), isolating the x ride-carry from the separate vertical-
    // carry caveat documented on `standing_crew_does_not_slide_off_a_vertically_moving_hull` below.
    let (mut s, t) = seated();
    let crew_x = SHIP_HOME.x - 60.0; // off-center on the bowl floor, clear of the seated pilot
    let crew_y =
        ink_floor_y_near(&s.paths[SHIP_SLOT], crew_x, s.fighters[0].pos.y, &s.nodes).unwrap();
    s.fighters[1].pos = Vector2::new(crew_x, crew_y);
    s.fighters[1].vel = Vector2::ZERO;
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_ink = SHIP_SLOT as i8;
    s.fighters[1].ground_plat = 0;
    let boost = InputFrame {
        cx: 1.0,
        attack: true,
        attack_held: true,
        ..IDLE
    };
    let start_hull_x = s.paths[SHIP_SLOT].pos.x;
    for f in 0..40 {
        s = step(&s, &[&boost, &IDLE], &t);
        assert_eq!(
            s.paths[SHIP_SLOT].vel.y, 0.0,
            "pure +x aim: hull never picks up y-vel"
        );
        assert_eq!(
            s.fighters[1].ground_ink, SHIP_SLOT as i8,
            "standing crew stays grounded on the hull at frame {f}"
        );
        let hull_dx = s.paths[SHIP_SLOT].pos.x - start_hull_x;
        let crew_dx = s.fighters[1].pos.x - crew_x;
        // ride-carry reads last frame's already-integrated surf vel (the FSM phase runs before
        // this frame's `integrate_ink`), so under SUSTAINED acceleration the gap is exactly one
        // frame's worth of the hull's current velocity -- bounded, not runaway. A constant-vel
        // surface (the mover, `surf_vel_tests`) has zero gap for the same reason this one doesn't.
        let bound = s.paths[SHIP_SLOT].vel.x.abs() + 0.25;
        assert!(
            (hull_dx - crew_dx).abs() <= bound,
            "standing crew's x tracks the flying hull within one frame's vel at {f}: hull moved {hull_dx}, crew moved {crew_dx} (bound {bound})"
        );
    }
    assert!(
        s.paths[SHIP_SLOT].pos.x - start_hull_x > 5.0,
        "the hull actually flew: {:?}",
        s.paths[SHIP_SLOT].pos
    );
}

#[test]
fn standing_crew_does_not_slide_off_a_vertically_moving_hull() {
    // Known limitation, documented rather than papered over: traveling ink integrates at the
    // BOTTOM of `step()` (after the FSM phase reads its position), unlike the kinematic mover
    // fixture (`drive_mover`, which runs before the FSM every frame) -- so a standing rider's
    // Y-pin reads the hull's position from one frame before its own vertical thrust lands. That
    // shows up as a small, CONSTANT lag (magnitude = the hull's own per-frame vertical speed),
    // never a growing drift and never a fall -- the bar this test pins. A real fix would mean
    // either reordering ink integration ahead of the FSM phase (a bigger change touching every
    // traveling-ink consumer) or relaxing `body::on_real_floor`'s stale-soup invariant for moving
    // surfaces; both are out of this step's "narrowest thing" scope.
    let t = tune();
    let mut s = SimState::spawn();
    let x = SHIP_HOME.x;
    let y = ink_floor_y_at(&s.paths[SHIP_SLOT], x, &s.nodes).expect("bowl floor spans dead center");
    s.fighters[0].pos = Vector2::new(x, y);
    s.fighters[0].vel = Vector2::ZERO;
    s.fighters[0].state = CharState::Stand;
    s.fighters[0].ground_ink = SHIP_SLOT as i8;
    s.fighters[0].ground_plat = 0;
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y);
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_plat = 0;
    // bypass thrust ramp-up: a constant vertical drift isolates the y-carry question from the
    // accelerating-velocity case (already covered above).
    s.paths[SHIP_SLOT].vel = Vector2::new(0.0, -3.0); // rising, native px/frame
    for f in 0..30 {
        s = step(&s, &[&IDLE, &IDLE], &t);
        assert_eq!(
            s.fighters[0].ground_ink, SHIP_SLOT as i8,
            "standing crew must not slide/fall off a vertically moving hull (frame {f})"
        );
        let gap = (s.fighters[0].pos.y - s.paths[SHIP_SLOT].pos.y).abs();
        assert!(
            gap < 400.0,
            "lag stays bounded (not a runaway drift) at frame {f}: gap={gap}"
        );
    }
}
