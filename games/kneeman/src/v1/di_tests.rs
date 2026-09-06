// Split out of lib.rs; declared as a crate-root child so `super::*` still means the crate root.

use super::*;

// Holding right while launched straight up should bend the trajectory toward +x by di_max_angle,
// leaving the speed untouched (survival DI steers the angle, never the magnitude).
#[test]
fn di_rotates_angle_keeps_speed() {
    let up = Vector2::new(0.0, -100.0); // screen-up launch
    let out = apply_di(up, Vector2::new(1.0, 0.0), 18.0);
    assert!(
        (out.length() - 100.0).abs() < 1e-3,
        "speed must be preserved"
    );
    assert!(out.x > 0.0, "stick right bends the launch toward +x");
    let deg = (out.x).atan2(-out.y).to_degrees(); // angle off vertical
    assert!(
        (deg - 18.0).abs() < 0.5,
        "rotation should hit the 18 deg cap, got {deg}"
    );
}

// Neutral stick (and stick inside the deadzone) leaves the trajectory alone.
#[test]
fn di_neutral_is_identity() {
    let v = Vector2::new(40.0, -90.0);
    assert_eq!(apply_di(v, Vector2::ZERO, 18.0), v);
    assert_eq!(apply_di(v, Vector2::new(0.1, 0.1), 18.0), v); // below the 0.3 deadzone
}

// Walked off the lip (Air + coyote window) and pressed jump: it's the GROUNDED jump
// (fullhop velocity), it does NOT spend the air jump, and the window closes.
#[test]
fn coyote_jump_is_full_and_keeps_air_jump() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::Air;
    f.pos = Vector2::new(600.0, 200.0); // airborne over center, far from any platform/ledge
    f.vel = Vector2::new(0.0, 40.0); // falling
    f.air_jumps = 1;
    f.coyote = 4;
    let jump = InputFrame {
        jump: true,
        jump_held: true,
        ..Default::default()
    };
    let idle = InputFrame::default();
    let out = step(&s, &[&jump, &idle], &t);
    let g = &out.fighters[0];
    // one frame of gravity is integrated after takeoff, so compare against fullhop + g*DT.
    let expect = t.fullhop_v + t.gravity * DT;
    assert!(
        (g.vel.y - expect).abs() < 1e-3,
        "coyote jump uses fullhop velocity"
    );
    assert_eq!(g.air_jumps, 1, "coyote jump must not consume the air jump");
    assert_eq!(g.coyote, 0, "the grace window closes after the jump");
}

// Same airborne state but the window has expired: jump spends the air jump instead.
#[test]
fn expired_coyote_spends_air_jump() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::Air;
    f.pos = Vector2::new(600.0, 200.0);
    f.vel = Vector2::new(0.0, 40.0);
    f.air_jumps = 1;
    f.coyote = 0;
    let jump = InputFrame {
        jump: true,
        jump_held: true,
        ..Default::default()
    };
    let idle = InputFrame::default();
    let out = step(&s, &[&jump, &idle], &t);
    let g = &out.fighters[0];
    let expect = t.airjump_v + t.gravity * DT;
    assert!(
        (g.vel.y - expect).abs() < 1e-3,
        "no coyote -> air jump velocity"
    );
    assert_eq!(g.air_jumps, 0, "air jump is consumed");
}

// Reversing a dash flips facing immediately but must NOT teleport velocity: the old momentum
// bleeds through 0 at dash_turn_accel over a few frames (continuous, Melee-style).
#[test]
fn dash_reversal_keeps_momentum_then_crosses() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::Dash;
    f.facing = 1.0;
    f.ground_plat = 0;
    f.pos = Vector2::new(600.0, GROUND_Y); // mid main floor, won't walk off
    f.vel = Vector2::new(t.run_speed, 0.0); // moving right at run speed
    let left = InputFrame {
        dir: -1.0,
        ..Default::default()
    };
    let idle = InputFrame::default();

    // frame 1: facing flips, but velocity is still rightward (no instant reversal).
    let s1 = step(&s, &[&left, &idle], &t);
    assert_eq!(
        s1.fighters[0].facing, -1.0,
        "facing flips on the reversal frame"
    );
    assert!(
        s1.fighters[0].vel.x > 0.0,
        "velocity must NOT teleport to the new direction"
    );

    // hold left a few more frames: momentum bleeds through 0 and goes negative.
    let mut cur = s1;
    for _ in 0..8 {
        cur = step(&cur, &[&left, &idle], &t);
    }
    assert!(
        cur.fighters[0].vel.x < 0.0,
        "sustained reversal eventually crosses 0 to the left"
    );
}

// Pressing special with the stick up enters up-B, launches the fighter upward, and finishes in
// Helpless (special-fall) once it's airborne.
#[test]
fn up_special_rises_then_helpless() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::Stand;
    f.ground_plat = 0;
    f.pos = Vector2::new(600.0, GROUND_Y);
    f.vel = Vector2::ZERO;
    let up_b = InputFrame {
        special: true,
        aim_y: -1.0,
        ..Default::default()
    };
    let hold = InputFrame {
        aim_y: -1.0,
        ..Default::default()
    };

    // press frame enters the up-B state
    let mut cur = step(&s, &[&up_b, &Default::default()], &t);
    assert_eq!(
        cur.fighters[0].state,
        CharState::SpecialU,
        "stick-up special = up-B"
    );

    // run it out: it leaves the ground rising, then becomes Helpless
    let mut saw_rise = false;
    let mut saw_helpless = false;
    for _ in 0..60 {
        cur = step(&cur, &[&hold, &Default::default()], &t);
        if cur.fighters[0].vel.y < 0.0 {
            saw_rise = true;
        }
        if cur.fighters[0].state == CharState::Helpless {
            saw_helpless = true;
            break;
        }
    }
    assert!(saw_rise, "up-B should drive the fighter upward");
    assert!(saw_helpless, "up-B ends in Helpless while airborne");
}

// Every B special must MOVE by integrating velocity, never by snapping position. This guards the
// reported "B teleports me": the largest legit burst is up-B at ~24px/frame, so any single-frame
// jump past 40px would be a teleport bug (a one-frame position write). The visible "blink" with
// the static test characters is missing air animation, not a position snap -- this proves it.
#[test]
fn specials_never_teleport() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    const MAX_STEP: f32 = 40.0;
    for (aim_y, dir, label) in [
        (-1.0f32, 0.0f32, "up-B"),
        (0.0, 1.0, "side-B"),
        (1.0, 0.0, "down-B"),
    ] {
        let mut s = SimState::spawn();
        {
            let f = &mut s.fighters[0];
            f.state = CharState::Stand;
            f.ground_plat = 0;
            f.pos = Vector2::new(600.0, GROUND_Y);
            f.vel = Vector2::ZERO;
        }
        let press = InputFrame {
            special: true,
            aim_y,
            dir,
            ..Default::default()
        };
        let hold = InputFrame {
            aim_y,
            dir,
            ..Default::default()
        };
        let mut cur = step(&s, &[&press, &Default::default()], &t);
        let mut prev = cur.fighters[0].pos;
        for _ in 0..40 {
            cur = step(&cur, &[&hold, &Default::default()], &t);
            let p = cur.fighters[0].pos;
            let d = (p - prev).length();
            assert!(
                d <= MAX_STEP,
                "{label}: single-frame jump {d:.1}px > {MAX_STEP} (teleport)"
            );
            prev = p;
        }
    }
}

// Neutral-B from standing enters the planted punch and stays grounded (no launch).
#[test]
fn neutral_special_is_grounded_punch() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::Stand;
    f.ground_plat = 0;
    f.pos = Vector2::new(600.0, GROUND_Y);
    let nb = InputFrame {
        special: true,
        ..Default::default()
    };
    let cur = step(&s, &[&nb, &Default::default()], &t);
    assert_eq!(
        cur.fighters[0].state,
        CharState::SpecialN,
        "neutral stick + special = neutral-B"
    );
    // a few frames in, still grounded and still in the punch (not launched into the air)
    let mut c = cur;
    for _ in 0..6 {
        c = step(&c, &[&Default::default(), &Default::default()], &t);
    }
    assert!(c.fighters[0].ground_plat >= 0, "neutral-B stays grounded");
}

// A special pressed in the air must STAY airborne. Regression for stale ground_plat (lingering
// from a jump) making run_special + the integrator treat the move as grounded — which planted
// the punch and snapped pos.y to the platform ("B-air teleports me to ground").
#[test]
fn aerial_special_stays_airborne() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    for (aim_y, dir, label) in [
        (0.0f32, 0.0f32, "N-air"),
        (0.0, 1.0, "side-air"),
        (-1.0, 0.0, "up-air"),
        (1.0, 0.0, "down-air"),
    ] {
        let mut s = SimState::spawn();
        {
            let f = &mut s.fighters[0];
            f.state = CharState::Air;
            f.ground_plat = 0; // stale grounded index lingering from a jump
            f.pos = Vector2::new(600.0, GROUND_Y - 200.0);
            f.vel = Vector2::ZERO;
        }
        let press = InputFrame {
            special: true,
            aim_y,
            dir,
            ..Default::default()
        };
        let cur = step(&s, &[&press, &Default::default()], &t);
        assert!(
            cur.fighters[0].pos.y < GROUND_Y - 50.0,
            "{label}: snapped to ground"
        );
        assert_eq!(
            cur.fighters[0].ground_plat, -1,
            "{label}: must read as airborne"
        );
    }
}

#[test]
fn grab_catches_holds_and_throws() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    // two fighters face-to-face, grounded, within grab range.
    for (k, x, face) in [(0usize, 600.0_f32, 1.0_f32), (1, 600.0 + 80.0, -1.0)] {
        let f = &mut s.fighters[k];
        f.state = CharState::Stand;
        f.ground_plat = 0;
        f.pos = Vector2::new(x, GROUND_Y);
        f.facing = face;
    }
    let grab = InputFrame {
        grab: true,
        ..Default::default()
    };
    let idle = InputFrame::default();

    // p0 presses grab; within startup+active it should catch p1.
    let mut c = step(&s, &[&grab, &idle], &t);
    for _ in 0..(t.grab_startup + t.grab_active) {
        c = step(&c, &[&idle, &idle], &t);
    }
    assert_eq!(c.fighters[0].state, CharState::GrabHold, "grabber holds");
    assert_eq!(c.fighters[1].state, CharState::Grabbed, "victim held");
    assert_eq!(c.fighters[0].grab_link, 1);
    assert_eq!(c.fighters[1].grab_link, 0);

    // pummel raises the victim's damage without releasing.
    let pummel = InputFrame {
        attack: true,
        ..Default::default()
    };
    let before = c.fighters[1].damage;
    c = step(&c, &[&pummel, &idle], &t);
    assert!(c.fighters[1].damage > before, "pummel deals damage");
    assert_eq!(
        c.fighters[0].state,
        CharState::GrabHold,
        "still holding after pummel"
    );

    // clear the grab-hold-grace window (idle stick, so nothing throws yet) before checking that
    // a bare stick deflection -- no button -- is what fires the throw now.
    for _ in 0..=GRAB_HOLD_GRACE {
        c = step(&c, &[&idle, &idle], &t);
    }
    assert_eq!(
        c.fighters[0].state,
        CharState::GrabHold,
        "idle stick past the grace still doesn't throw"
    );

    // forward stick deflection alone = throw: victim launched, both unlinked.
    let fwd = InputFrame {
        dir: 1.0, // same side as the grabber's facing -> fthrow
        ..Default::default()
    };
    c = step(&c, &[&fwd, &idle], &t);
    assert_eq!(c.fighters[0].grab_link, -1, "grabber released on throw");
    assert!(c.fighters[1].hitstun > 0, "victim launched with hitstun");
    assert_ne!(
        c.fighters[1].state,
        CharState::Grabbed,
        "victim no longer held"
    );
}

#[test]
fn grab_whiffs_to_neutral_when_out_of_range() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    let f0 = &mut s.fighters[0];
    f0.state = CharState::Stand;
    f0.ground_plat = 0;
    f0.pos = Vector2::new(300.0, GROUND_Y);
    f0.facing = 1.0;
    let f1 = &mut s.fighters[1];
    f1.state = CharState::Stand;
    f1.ground_plat = 0;
    f1.pos = Vector2::new(900.0, GROUND_Y); // far away
    let grab = InputFrame {
        grab: true,
        ..Default::default()
    };
    let idle = InputFrame::default();
    let mut c = step(&s, &[&grab, &idle], &t);
    assert_eq!(c.fighters[0].state, CharState::Grab, "entered grab");
    for _ in 0..(t.grab_startup + t.grab_active + t.grab_recovery + 1) {
        c = step(&c, &[&idle, &idle], &t);
    }
    assert_eq!(
        c.fighters[0].state,
        CharState::Stand,
        "whiffed grab returns to neutral"
    );
    assert_eq!(c.fighters[1].state, CharState::Stand, "victim untouched");
}

/// Helper: a fighter hovering one frame above the floor, launched downward in tumble.
fn launched(t: &Tune) -> SimState {
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::Air;
    f.ground_plat = -1;
    f.pos = Vector2::new(600.0, GROUND_Y - 5.0);
    f.vel = Vector2::new(120.0, 600.0); // moving down hard: crosses the floor next frame
    f.hitstun = 30;
    f.tumble = true;
    let _ = t;
    s
}

#[test]
fn hard_launch_knocks_down_without_tech() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let s = launched(&t);
    let idle = InputFrame::default();
    let c = step(&s, &[&idle, &idle], &t);
    assert_eq!(
        c.fighters[0].state,
        CharState::Knockdown,
        "missed tech -> floored"
    );
    assert!(!c.fighters[0].tumble, "tumble cleared on knockdown");
}

#[test]
fn launched_victim_falls_back_to_ground() {
    // A fighter popped up with hitstun must arc back down under gravity, not hover. Regression
    // for the "opponent floats after a hit" bug.
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::Air;
    f.ground_plat = -1;
    f.pos = Vector2::new(600.0, GROUND_Y);
    f.vel = Vector2::new(0.0, -900.0); // straight up (won't blast off the top: apex ~95px)
    f.hitstun = 24;
    let idle = InputFrame::default();
    let mut c = s;
    let apex = {
        let mut hi = c.fighters[0].pos.y;
        for _ in 0..120 {
            c = step(&c, &[&idle, &idle], &t);
            hi = hi.min(c.fighters[0].pos.y); // smaller y = higher
        }
        hi
    };
    assert!(
        apex < GROUND_Y - 50.0,
        "victim actually rose off the launch"
    );
    assert!(
        c.fighters[0].pos.y >= GROUND_Y - 1.0,
        "victim fell back to the floor (no hover): y={}",
        c.fighters[0].pos.y
    );
}

#[test]
fn aerial_neutral_b_does_not_air_stall() {
    // Neutral-B in the air must impulse + fall, not hover in place. Regression for Falcon-B
    // hovering; also pins the forward impulse direction.
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::SpecialN;
    f.frame = 0;
    f.ground_plat = -1; // airborne
    f.facing = 1.0;
    f.pos = Vector2::new(600.0, GROUND_Y - 600.0); // high up, room to fall
    f.vel = Vector2::ZERO;
    let idle = InputFrame::default();
    let start_y = f.pos.y;
    let mut c = s;
    // run past the launch frame (Punch startup) plus a bit
    for _ in 0..30 {
        c = step(&c, &[&idle, &idle], &t);
    }
    assert!(
        c.fighters[0].pos.y > start_y + 100.0,
        "aerial neutral-B descends instead of hovering: dy={}",
        c.fighters[0].pos.y - start_y
    );
    assert!(
        c.fighters[0].vel.x.abs() > 1.0 || c.fighters[0].pos.x > 600.0,
        "neutral-B carries a forward impulse, not a planted stall"
    );
}

#[test]
fn shield_at_impact_techs_in_place() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let s = launched(&t);
    let tech = InputFrame {
        shield_pressed: true,
        ..Default::default()
    };
    let idle = InputFrame::default();
    let c = step(&s, &[&tech, &idle], &t);
    assert_eq!(
        c.fighters[0].state,
        CharState::TechInPlace,
        "teched the landing"
    );
    assert!(c.fighters[0].intangible, "tech is intangible");
}

#[test]
fn directional_tech_rolls() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let s = launched(&t);
    let tech_left = InputFrame {
        shield_pressed: true,
        dir: -1.0,
        ..Default::default()
    };
    let idle = InputFrame::default();
    let c = step(&s, &[&tech_left, &idle], &t);
    assert_eq!(
        c.fighters[0].state,
        CharState::TechRoll,
        "held a direction -> tech roll"
    );
    assert!(c.fighters[0].vel.x < 0.0, "rolls in the held direction");
}

#[test]
fn knockdown_auto_getups_to_stand() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let s = launched(&t);
    let idle = InputFrame::default();
    let mut c = step(&s, &[&idle, &idle], &t);
    assert_eq!(c.fighters[0].state, CharState::Knockdown);
    for _ in 0..(t.knockdown_frames + t.getup_frames + 2) {
        c = step(&c, &[&idle, &idle], &t);
    }
    assert_eq!(
        c.fighters[0].state,
        CharState::Stand,
        "floored -> getup -> stand"
    );
}

// Launched past the lip: hitstun does NOT gate a ledge catch (directed 2026-07-04).
// Falling through the snap window mid-hitstun grabs the ledge, and the catch IS the
// recovery -- stun and tumble are consumed by the mechanic.
#[test]
fn hitstun_fall_past_the_lip_still_catches_the_ledge() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::Air;
    f.ground_plat = -1;
    f.pos = Vector2::new(FLOOR_RIGHT + 30.0, GROUND_Y + 10.0); // off the right side, in the window
    f.vel = Vector2::new(0.0, 400.0); // falling well past LEDGE_FALL_EPS
    f.hitstun = 30;
    f.tumble = true;
    let idle = InputFrame::default();
    let c = step(&s, &[&idle, &idle], &t);
    let f = c.fighters[0];
    assert_eq!(
        f.state,
        CharState::LedgeHold,
        "caught the ledge mid-hitstun"
    );
    assert_eq!(f.hitstun, 0, "the catch consumed the stun");
    assert!(!f.tumble, "tumble cleared by the catch");
    assert_eq!(f.vel, Vector2::ZERO, "hanging still");
}
