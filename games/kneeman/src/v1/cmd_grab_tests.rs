// Falcon up-B stationary command grab (queue-2026-07-03 item 4): the air-grab connect, the
// fixed-delay latch, the fixed-angle un-DI-able explosion, the whiff recovery, and the char gate.
// Declared as a crate-root child so `super::*` is the crate root (foundations_tests.rs convention).

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

/// A match where every roster row is Falcon: `char_id` 0 and 1 both get the command-grab up-B.
fn falcon_tune() -> Tune {
    Tune::resolve(&CharSpec::falcon(), &MatchTune::default())
}

/// P0 airborne mid-up-B (`SpecialU` running Falcon's `DiveGrab`) at `g.frame == frame`, facing
/// right; P1 airborne at `vx`. `frame` is the PRE-step frame -- `reduce_next_state` ticks it once
/// before `resolve_grab` reads it, so the hug window [10, 26) is live for a pre-step frame in [9, 25).
fn diving_pair(frame: i64, gx: f32, vx: f32) -> SimState {
    let mut s = SimState::spawn();
    let g = &mut s.fighters[0];
    g.state = CharState::SpecialU;
    g.frame = frame;
    g.pos = Vector2::new(gx, 300.0);
    g.vel = Vector2::ZERO;
    g.facing = 1.0;
    g.ground_plat = -1;
    g.grab_link = -1;
    let v = &mut s.fighters[1];
    v.state = CharState::Air;
    v.pos = Vector2::new(vx, 300.0);
    v.vel = Vector2::ZERO;
    v.facing = -1.0;
    v.ground_plat = -1;
    s
}

/// P0 already latched onto P1 as a COMMAND grab (`GrabHold` + `dive_latch`), `timer` frames from the
/// explosion; P1 held (`Grabbed`) at `pct` damage. Built directly so a test can sit on either side
/// of the boom frame without stepping through the hug window first.
fn command_latch(timer: i64, pct: f32) -> SimState {
    let mut s = SimState::spawn();
    let g = &mut s.fighters[0];
    g.state = CharState::GrabHold;
    g.dive_latch = true;
    g.frame = 5;
    g.pos = Vector2::new(600.0, 300.0);
    g.facing = 1.0;
    g.ground_plat = -1;
    g.grab_link = 1;
    g.grab_timer = timer;
    let v = &mut s.fighters[1];
    v.state = CharState::Grabbed;
    v.pos = Vector2::new(600.0, 300.0);
    v.facing = -1.0;
    v.ground_plat = -1;
    v.grab_link = 0;
    v.grab_timer = timer;
    v.damage = pct;
    s
}

// ── the general air-grab unlock (item 1) + Falcon connect (item 2) ────────────────────────────

#[test]
fn air_grab_connects_when_both_airborne() {
    let t = falcon_tune();
    let s = diving_pair(12, 600.0, 680.0); // 80px apart: inside the hug circle (r 92 + hurt 48)
    let victim_di = InputFrame {
        aim_y: -1.0,
        ..IDLE
    }; // victim mashing a direction: irrelevant to the catch
    let c = step(&s, &[&IDLE, &victim_di], &t);
    assert_eq!(
        c.fighters[0].state,
        CharState::GrabHold,
        "grabber latched from the air"
    );
    assert!(c.fighters[0].dive_latch, "flagged as a command latch");
    assert_eq!(c.fighters[0].grab_link, 1);
    assert_eq!(
        c.fighters[1].state,
        CharState::Grabbed,
        "victim caught while airborne"
    );
    assert_eq!(c.fighters[1].grab_link, 0);
    assert_eq!(c.fighters[0].vel, Vector2::ZERO, "grabber hangs in place");
    assert_eq!(
        c.fighters[1].vel,
        Vector2::ZERO,
        "victim frozen (both hang in place)"
    );
}

#[test]
fn ground_grab_is_unchanged_by_the_air_case() {
    let t = falcon_tune();
    // a NORMAL grounded grab (CharState::Grab, not the up-B): must still flip to a plain GrabHold
    // with no `dive_latch` -- the existing catch path is untouched.
    let mut s = SimState::spawn();
    let g = &mut s.fighters[0];
    g.state = CharState::Grab;
    g.frame = t.grab_startup; // reduce ticks into the active window
    g.pos = Vector2::new(600.0, GROUND_Y);
    g.ground_plat = 0;
    g.facing = 1.0;
    g.grab_link = -1;
    let v = &mut s.fighters[1];
    v.pos = Vector2::new(600.0 + t.grab_range * 0.5, GROUND_Y);
    v.ground_plat = 0;
    let c = step(&s, &[&IDLE, &IDLE], &t);
    assert_eq!(
        c.fighters[0].state,
        CharState::GrabHold,
        "normal grab holds"
    );
    assert!(
        !c.fighters[0].dive_latch,
        "a normal grab is NOT a command latch"
    );
    assert_eq!(c.fighters[1].state, CharState::Grabbed);
}

// ── the latch -> fixed-angle explosion (items 3, 4) ───────────────────────────────────────────

#[test]
fn command_latch_freezes_then_explodes_on_the_timer_frame() {
    let t = falcon_tune();
    // timer 2: frozen (no boom) on step 1, explode on step 2 (resolve_grab decrements once/step).
    let mut latched = command_latch(2, 40.0);
    latched.fighters[0].air_jumps = 0; // spent before the dive: proves the connect restores it
    let c1 = step(&latched, &[&IDLE, &IDLE], &t);
    assert_eq!(
        c1.fighters[0].state,
        CharState::GrabHold,
        "still latched at timer 1"
    );
    assert_eq!(
        c1.fighters[1].state,
        CharState::Grabbed,
        "victim still held"
    );
    assert_eq!(c1.fighters[1].vel, Vector2::ZERO, "no launch yet");

    let c2 = step(&c1, &[&IDLE, &IDLE], &t);
    assert_eq!(
        c2.fighters[0].state,
        CharState::Air,
        "a CONNECTED dive flips away actionable, not Helpless (that's the whiff)"
    );
    assert!(
        c2.fighters[0].vel.y < 0.0,
        "flyaway hop rises, vel.y={}",
        c2.fighters[0].vel.y
    );
    assert!(
        c2.fighters[0].vel.x < 0.0,
        "flyaway drifts away from the facing (facing 1.0), vel.x={}",
        c2.fighters[0].vel.x
    );
    assert_eq!(
        c2.fighters[0].air_jumps, t.max_air_jumps as u8,
        "the air jump restores on a successful dive"
    );
    assert!(!c2.fighters[0].dive_latch, "latch flag cleared");
    assert_eq!(c2.fighters[1].state, CharState::Launched, "victim launched");
    assert!(c2.fighters[1].hitstun > 0, "victim in hitstun");
    assert!(c2.fighters[1].vel.length() > 0.0, "victim flies");
    assert!(c2.fighters[1].damage > 40.0, "explosion dealt its %");
    assert!(
        c2.fx
            .iter()
            .any(|f| f.kind == FxKind::Fire && f.tick == c2.tick),
        "cheesy fireball emitted at the boom frame"
    );
}

#[test]
fn launch_angle_is_fixed_under_opposite_di_and_speed_scales_with_percent() {
    let t = falcon_tune();
    // opposite DI holds on the boom step must produce the SAME launch angle (no DI bend).
    let di_up = InputFrame {
        aim_y: -1.0,
        dir: 1.0,
        ..IDLE
    };
    let di_down = InputFrame {
        aim_y: 1.0,
        dir: -1.0,
        ..IDLE
    };
    let a = step(&command_latch(1, 40.0), &[&IDLE, &di_up], &t);
    let b = step(&command_latch(1, 40.0), &[&IDLE, &di_down], &t);
    let (va, vb) = (a.fighters[1].vel, b.fighters[1].vel);
    let ang_a = va.y.atan2(va.x);
    let ang_b = vb.y.atan2(vb.x);
    assert!(
        (ang_a - ang_b).abs() < 1e-4,
        "launch angle identical under opposite DI: {ang_a} vs {ang_b}"
    );
    assert!(
        va.x > 0.0 && va.y < 0.0,
        "fixed 45deg up-and-away, mirrored by the attacker's (right) facing"
    );

    // percent scales the SPEED through the normal formula, but not the angle.
    let hi = step(&command_latch(1, 120.0), &[&IDLE, &IDLE], &t);
    let vhi = hi.fighters[1].vel;
    assert!(
        vhi.length() > va.length(),
        "higher % -> more launch speed ({} vs {})",
        vhi.length(),
        va.length()
    );
    let ang_hi = vhi.y.atan2(vhi.x);
    assert!(
        (ang_hi - ang_a).abs() < 1e-4,
        "angle unchanged by percent: {ang_hi} vs {ang_a}"
    );
}

#[test]
fn facing_mirrors_the_fixed_launch_angle() {
    let t = falcon_tune();
    let mut s = command_latch(1, 40.0);
    s.fighters[0].facing = -1.0; // grabber faces left: the fixed angle mirrors up-and-LEFT
    let c = step(&s, &[&IDLE, &IDLE], &t);
    let v = c.fighters[1].vel;
    assert!(
        v.x < 0.0 && v.y < 0.0,
        "left facing launches up-and-away left"
    );
}

// ── whiff recovery (item 2) ───────────────────────────────────────────────────────────────────

#[test]
fn command_grab_whiff_recovers_to_helpless() {
    let t = falcon_tune();
    // grabber in the up-B window, victim far out of the hug circle: the window elapses uncaught.
    let mut c = diving_pair(9, 400.0, 1000.0);
    for _ in 0..70 {
        c = step(&c, &[&IDLE, &IDLE], &t);
        if c.fighters[0].state != CharState::SpecialU {
            break;
        }
    }
    assert_eq!(
        c.fighters[0].state,
        CharState::Helpless,
        "a whiffed command grab special-falls (Helpless)"
    );
    assert_eq!(c.fighters[0].grab_link, -1, "nothing caught");
    assert!(!c.fighters[0].dive_latch, "no latch armed");
}

// ── char gating (only the Falcon row) ─────────────────────────────────────────────────────────

#[test]
fn up_special_is_a_command_grab_only_for_the_falcon_row() {
    // char_id 0 = Falcon, char_id 1 = KneeMan (Rise up-B). Row 0 IS the flat view (`for_char(0)`
    // returns `*self`), so Falcon's up-B goes on the flat `specials`; row 1 re-resolves from the
    // roster, which stays KneeMan.
    let mut t = Tune::from_char(&CharData::KNEEMAN);
    t.specials[2] = SpecialMove::FALCON_DIVE;

    let mut s = SimState::spawn();
    for f in s.fighters.iter_mut().take(2) {
        f.state = CharState::Air;
        f.vel = Vector2::ZERO;
        f.ground_plat = -1;
        f.pos.y = 300.0;
    }
    s.fighters[0].pos.x = 400.0; // Falcon
    s.fighters[0].char_id = 0;
    s.fighters[1].pos.x = 800.0; // KneeMan, far enough that neither catches the other
    s.fighters[1].char_id = 1;

    let upb = InputFrame {
        aim_y: -1.0,
        special: true,
        ..IDLE
    };
    let c1 = step(&s, &[&upb, &upb], &t); // press -> both enter SpecialU
    assert_eq!(c1.fighters[0].state, CharState::SpecialU, "Falcon in up-B");
    assert_eq!(c1.fighters[1].state, CharState::SpecialU, "KneeMan in up-B");

    let c2 = step(&c1, &[&IDLE, &IDLE], &t); // run_special routes each by its loadout kind
    assert_eq!(
        c2.fighters[0].vel,
        Vector2::ZERO,
        "Falcon up-B is the STATIONARY command grab (no travel)"
    );
    assert!(
        c2.fighters[1].vel.y < -100.0,
        "KneeMan up-B is the Rise recovery: launches upward, not a command grab"
    );
}
