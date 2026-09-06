// Ship-hull playtest round (2026-07-05): the purple equator is one-way (an inside launch
// escapes OUT through it, still blocked from outside) and the yellow hatch lips are grabbable
// by a launched/recovering fighter (the plain crew-drop still falls straight through the hatch).
// Crate-root child so `super::*` is the crate root.

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

/// Fighter 1 parked far away on the main stage, idle, so no combat ever fires.
fn park_second(s: &mut SimState) {
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y);
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_plat = 0;
}

// ── Task 1: purple equator is SOLID both ways, with bounce (plans/ship-containment.md #1) ──────────
// 2026-07-06: SUPERSEDES the 97d11f5 one-way-purple decision -- the user's spec is a solid bouncy
// container ("once your ECB crosses me from my back, you cannot come through this direction, at
// all"), exit only via the hatch gap.

#[test]
fn inside_launch_blocked_and_bounced_by_the_solid_equator() {
    // A body launched from INSIDE the bowl straight at the right equator wall (segs 3/4,
    // Wall-classified, world x ~116, y 425..615) is blocked, never leaks out, and rebounds off the
    // hull's own bounce row (the "multi bounce in the ball" fantasy) instead of dead-stopping.
    // Start well inside (x=0, dist_from_center 190 < SHIP_R) at equator height, launched hard +x.
    let t = tune();
    let mut s = SimState::spawn();
    s.fighters[0].pos = Vector2::new(0.0, SHIP_HOME.y);
    s.fighters[0].vel = Vector2::new(900.0, 0.0);
    s.fighters[0].state = CharState::Air;
    s.fighters[0].hitstun = 60;
    s.fighters[0].tumble = true;
    s.fighters[0].ground_ink = -1;
    s.fighters[0].ground_plat = -1;
    park_second(&mut s);
    let start = (s.fighters[0].pos - SHIP_HOME).length();
    assert!(
        start < SHIP_R - 20.0,
        "sanity: must start inside, dist={start}"
    );
    let mut bounced_inward = false;
    for _ in 0..60 {
        s = step(&s, &[&IDLE, &IDLE], &t);
        let f = &s.fighters[0];
        assert!(
            (f.pos - SHIP_HOME).length() < SHIP_R + 20.0,
            "the solid equator must never let an inside launch escape: dist={} pos={:?}",
            (f.pos - SHIP_HOME).length(),
            f.pos
        );
        if f.vel.x < 0.0 {
            bounced_inward = true;
        }
    }
    assert!(
        bounced_inward,
        "the equator's own bounce row must rebound the launch back inward, final vel={:?}",
        s.fighters[0].vel
    );
}

#[test]
fn outside_launch_still_blocked_by_the_equator() {
    // A body arriving from OUTSIDE the equator is blocked, same as always -- both directions block
    // now (mirror of ship_contain_tests::hitstun_launch_into_dome_equator_wall_blocks, kept local
    // so a regression in this file's fix is caught here too).
    let t = tune();
    let mut s = SimState::spawn();
    s.fighters[0].pos = Vector2::new(SHIP_HOME.x + 400.0, SHIP_HOME.y);
    s.fighters[0].vel = Vector2::new(-600.0, 0.0);
    s.fighters[0].state = CharState::Air;
    s.fighters[0].hitstun = 60;
    s.fighters[0].tumble = true;
    s.fighters[0].ground_ink = -1;
    s.fighters[0].ground_plat = -1;
    park_second(&mut s);
    for _ in 0..60 {
        s = step(&s, &[&IDLE, &IDLE], &t);
    }
    assert!(
        (s.fighters[0].pos - SHIP_HOME).length() >= SHIP_R - 20.0,
        "the equator still blocks from outside: dist={} pos={:?}",
        (s.fighters[0].pos - SHIP_HOME).length(),
        s.fighters[0].pos
    );
}

#[test]
fn rising_up_through_the_hatch_gap_passes_never_blocked() {
    // The gap is not a wall (plans/ship-containment.md #1): only the purple Wall segments became
    // Solid-both-ways; the hatch opening itself carries no surf at all. A body launched straight UP
    // through the throat (dead-center x, clear of both flank lips) must sail out to open air above
    // the hull, confirming the solid conversion sealed the RIM, never the gap.
    let t = tune();
    let mut s = SimState::spawn();
    s.fighters[0].pos = Vector2::new(SHIP_HOME.x, SHIP_HOME.y - SHIP_R + 60.0); // inside, under the hatch
    s.fighters[0].vel = Vector2::new(0.0, -900.0); // straight up through the throat
    s.fighters[0].state = CharState::Air;
    s.fighters[0].ground_ink = -1;
    s.fighters[0].ground_plat = -1;
    park_second(&mut s);
    let mut cleared = false;
    for _ in 0..40 {
        s = step(&s, &[&IDLE, &IDLE], &t);
        if s.fighters[0].pos.y < SHIP_HOME.y - SHIP_R - 20.0 {
            cleared = true;
            break;
        }
    }
    assert!(
        cleared,
        "the hatch gap must never block an upward exit, final pos={:?}",
        s.fighters[0].pos
    );
}

// ── Task 2: yellow hatch lips grabbable, directional (plans/ledge-ship-fixes.md #1/#3) ─────────────
//
// The `allow_container` special case is gone: a container (hull hatch) lip flows through the SAME
// directional below+outboard box as any other lip. Entry and exit are separated by GEOMETRY, not
// launch state: a body above the lip (dropping in) is rejected by the ceiling gate; a body already
// below/beside it (rising up through the throat toward the opening) is admitted.

/// Below the right hatch lip (world ~(-95,229)), inside the throat, already in the catch band --
/// the position a body rising up out of the hull toward the opening passes through.
fn below_the_right_hatch_lip(s: &mut SimState) {
    s.fighters[0].pos = Vector2::new(-95.0, 273.0);
    s.fighters[0].vel = Vector2::new(0.0, 300.0); // falling fast enough to satisfy ledge_fall_eps
    s.fighters[0].state = CharState::Air;
    s.fighters[0].ground_ink = -1;
    s.fighters[0].ground_plat = -1;
}

#[test]
fn rising_from_inside_grabs_the_yellow_hatch_lip() {
    // The user's exact complaint (ledge-ship-fixes.md #3): "i dont get ledge grab out of the
    // ship" -- a fighter approaching the hatch lip from below/inside on the ORDINARY plain-Air
    // path (no hitstun, no `allow_container` flag) now grabs the exit. The lip rides the hull
    // (ledge_ink = SHIP_SLOT), so a flying hull would carry the hang.
    let t = tune();
    let mut s = SimState::spawn();
    below_the_right_hatch_lip(&mut s);
    park_second(&mut s);
    let mut caught = false;
    for _ in 0..20 {
        s = step(&s, &[&IDLE, &IDLE], &t);
        if s.fighters[0].state == CharState::LedgeHold {
            caught = true;
            break;
        }
    }
    assert!(
        caught,
        "a plain (non-launched) body rising toward the hatch must grab the yellow lip, state={:?} pos={:?}",
        s.fighters[0].state, s.fighters[0].pos
    );
    assert_eq!(
        s.fighters[0].ledge_ink, SHIP_SLOT as i8,
        "the hang rides the hull stroke so a flying hull carries it"
    );
}

#[test]
fn plain_crew_drop_falls_through_the_hatch_never_grabs() {
    // The disambiguation that preserves the container: a fighter dropped in dead-center over the
    // hatch gap, well ABOVE both flank lips, must fall straight through into the bowl -- the
    // ceiling gate (`dy < -ledge_ceil`) rejects every hatch lip while the body is still above it.
    let t = tune();
    let mut s = SimState::spawn();
    s.fighters[0].pos = Vector2::new(SHIP_HOME.x, SHIP_HOME.y - SHIP_R - 40.0);
    s.fighters[0].vel = Vector2::new(0.0, 300.0);
    s.fighters[0].state = CharState::Air;
    s.fighters[0].ground_ink = -1;
    s.fighters[0].ground_plat = -1;
    park_second(&mut s);
    for _ in 0..60 {
        s = step(&s, &[&IDLE, &IDLE], &t);
        assert_ne!(
            s.fighters[0].state,
            CharState::LedgeHold,
            "a plain crew drop must never grab the hatch lip, pos={:?}",
            s.fighters[0].pos
        );
    }
    assert_eq!(s.fighters[0].ledge_ink, -1, "never latched a container lip");
}
