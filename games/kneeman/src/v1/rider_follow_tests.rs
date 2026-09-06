// Rider-follow re-pin (plans/ship-containment.md row 4): a grounded/clinging rider's ride
// carry is computed by the FSM from a frame-start ink snapshot, one tick stale by the time
// `integrate_ink`/`resolve_ink_billiard` finish moving the ridden path THIS same tick. Split out
// of ship_tests.rs (both files sat at their .dl file-budget caps; this is the file-budget fix,
// not a topic split -- these two pin the row-4 fix directly, ship_tests.rs keeps the rest of the
// hull/station suite). Declared as a crate-root child so `super::*` is the crate root, same
// convention as every other `*_tests.rs`.

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

#[test]
fn standing_crew_catches_up_to_the_hulls_hard_bounce_off_the_top_blast_wall() {
    // plans/ship-containment.md row 4 direction A: the FSM's ground-carry (za_warudo.rs's
    // ground-ink branch, direction B below) reads the hull's velocity from the FRAME-START ink
    // snapshot and applies it ONCE; but `integrate_ink` can do more to the hull THIS SAME TICK
    // than that one linear add predicts -- the hard bounce off the invincible blast frame
    // (`stage::integrate_ink`'s `p.owner < 0` clamp+reflect, same rule the hull itself uses) is
    // the sharpest case: one tick, a position correction that doesn't match the raw pre-bounce
    // velocity at all. Without a post-ink re-pin the standing crew's y (re-derived every tick
    // from the STALE, frame-start dome geometry -- za_warudo's ground branch never actually adds
    // a y carry, it only widens its own continuity gate) silently keeps its pre-tick offset from
    // the hull instead of tracking the hull's true one-tick move -- exactly the "when the ball
    // moves it gets wonky" bug. Rig: the REAL hull (not a synthetic stroke, so this is the
    // containment mechanic itself), moved off its home to (600, -212) -- clear of every other
    // fixture, its bounding circle's top edge only ~12px shy of `BLAST_TOP` (-520) -- driven at
    // -4.9 px/frame straight up: under `INK_TRUCK_SPEED` (5, so the hull's own translation
    // doesn't ALSO register as a truck-hazard hit on the standing crew, a separate mechanic out
    // of row 4's scope) yet enough to cross the wall and bounce in this one tick.
    let t = tune();
    let mut s = SimState::spawn();
    s.paths[SHIP_SLOT].pos = Vector2::new(600.0, -212.0);
    s.paths[SHIP_SLOT].vel = Vector2::new(0.0, -4.9); // native px/frame, straight up
    let x = s.paths[SHIP_SLOT].pos.x;
    let y = ink_floor_y_at(&s.paths[SHIP_SLOT], x, &s.nodes).expect("bowl floor spans dead center");
    s.fighters[0].pos = Vector2::new(x, y);
    s.fighters[0].vel = Vector2::ZERO;
    s.fighters[0].state = CharState::Stand;
    s.fighters[0].ground_ink = SHIP_SLOT as i8;
    s.fighters[0].ground_plat = 0;
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y);
    s.fighters[1].state = CharState::Stand;
    s.fighters[1].ground_plat = 0;
    let before_hull_y = s.paths[SHIP_SLOT].pos.y;
    let before_crew_y = s.fighters[0].pos.y;
    let n = step(&s, &[&IDLE, &IDLE], &t);
    assert!(
        n.paths[SHIP_SLOT].vel.y > 0.0,
        "phase check: the hull actually bounced (vel reversed) this tick: {:?}",
        n.paths[SHIP_SLOT].vel
    );
    let hull_dy = n.paths[SHIP_SLOT].pos.y - before_hull_y;
    assert!(
        (hull_dy - (-4.9)).abs() > 1.0,
        "phase check: the bounce clamp made the hull's true this-tick delta ({hull_dy}) diverge \
         from the raw pre-bounce vel (-4.9) -- otherwise this test isn't exercising the bounce"
    );
    assert_eq!(
        n.fighters[0].ground_ink, SHIP_SLOT as i8,
        "the bounce must not shed the standing crew off the hull"
    );
    let crew_dy = n.fighters[0].pos.y - before_crew_y;
    assert!(
        (crew_dy - hull_dy).abs() < 0.1,
        "standing crew's y tracks the hull's TRUE post-bounce delta this same tick, not the \
         pre-bounce straight-line one: crew moved {crew_dy}, hull moved {hull_dy}"
    );
}

#[test]
fn standing_crew_stays_planted_through_sustained_vertical_hull_drive() {
    // plans/ship-containment.md row 4, the literal pinned gate: "fighter standing in the bowl,
    // hull driven vertically -> stays planted (does not drop to Air)". Companion to
    // `standing_crew_does_not_slide_off_a_vertically_moving_hull` (ship_tests.rs, that one pins
    // the bounded-lag invariant for a gentle drift); this one is the direct state-machine
    // assertion the plan asks for, run over enough frames that a regression silently dropping
    // the rider to `Air` (`ground_ink` reset to -1) fails loudly by name.
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
    s.paths[SHIP_SLOT].vel = Vector2::new(0.0, -12.0); // native px/frame, a brisk vertical drive
    for f in 0..60 {
        s = step(&s, &[&IDLE, &IDLE], &t);
        assert_ne!(
            s.fighters[0].state,
            CharState::Air,
            "standing crew must stay planted, not drop to Air, at frame {f}"
        );
        assert_eq!(
            s.fighters[0].ground_ink, SHIP_SLOT as i8,
            "standing crew stays grounded on the hull at frame {f}"
        );
    }
}
