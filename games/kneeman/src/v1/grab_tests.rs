// Air pickup (GRAB, not ATTACK) + thrown-item catching + the pickup capsule's platform-height
// tolerance. Declared as a crate-root child so `super::*` is the crate root (foundations_tests.rs's
// convention).

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

fn laser_gun_at(pos: Vector2) -> Item {
    Item {
        cell: None,
        kind: ItemKind::LaserGun,
        pos,
        vel: Vector2::ZERO,
        owner: -1,
        gas: 16.0,
        gas_max: 16.0,
        timer: 0,
        facing: 1.0,
        tool: ToolKind::TrailPen,
        stroke: 0,
        thrown: false,
        mount: -1,
        hp: 0.0,
    }
}

// ── air pickup (GRAB) vs. air attack (aerial) ────────────────────────────────

/// P0 airborne, a hair above a ground item directly beneath it; P1 stays out of the way.
fn airborne_over_item() -> (SimState, Tune) {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    s.fighters[0].state = CharState::Air;
    s.fighters[0].pos = Vector2::new(600.0, GROUND_Y - 40.0);
    s.fighters[0].vel = Vector2::ZERO;
    s.fighters[0].facing = 1.0;
    s.fighters[0].ground_plat = -1;
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y);
    s.items[0] = laser_gun_at(Vector2::new(600.0, GROUND_Y));
    (s, t)
}

#[test]
fn airborne_grab_picks_up_a_ground_item() {
    let (s, t) = airborne_over_item();
    let grab = InputFrame { grab: true, ..IDLE };
    let c = step(&s, &[&grab, &IDLE], &t);
    assert_eq!(
        c.fighters[0].holding, 0,
        "GRAB near a reachable item claims it in the air, same as on the ground"
    );
    assert_eq!(c.items[0].owner, 0);
    assert!(
        airborne(c.fighters[0].state),
        "picking up mid-air doesn't ground you"
    );
}

#[test]
fn airborne_attack_near_the_same_item_stays_an_aerial() {
    let (s, t) = airborne_over_item();
    let atk = InputFrame {
        attack: true,
        ..IDLE
    };
    let c = step(&s, &[&atk, &IDLE], &t);
    assert_eq!(
        c.fighters[0].holding, -1,
        "ATTACK in the air must NOT pick up -- that's the whole reason nearest_pickup gates airborne"
    );
    assert_eq!(c.items[0].owner, -1, "the item stays unowned on the ground");
    assert!(
        matches!(
            c.fighters[0].state,
            CharState::Nair | CharState::Fair | CharState::Bair | CharState::Uair | CharState::Dair
        ),
        "the press still reads as an aerial: got {:?}",
        c.fighters[0].state
    );
}

// ── platform-height tolerance on the pickup reach ────────────────────────────

#[test]
fn pickup_reaches_an_item_on_a_platform_above_within_tolerance() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut f = Fighter::spawn(600.0, 1.0);
    f.state = CharState::Stand;
    f.pos = Vector2::new(600.0, GROUND_Y);
    f.ground_plat = 0;
    let mut items = [Item::EMPTY; MAX_ITEMS];
    // PLATFORMS[1]: a side soft platform, 185px above the main floor -- inside the pickup
    // box's vertical half-height (PICKUP_VERT_TOL), so one platform-drop up is in reach.
    items[0] = laser_gun_at(Vector2::new(600.0, PLATFORMS[1].y));
    assert!(
        nearest_pickup(&f, &items, &t).is_some(),
        "an item one platform-drop above the body should still be in reach"
    );
}

#[test]
fn pickup_reaches_an_item_behind_the_fighter() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut f = Fighter::spawn(600.0, 1.0); // facing RIGHT
    f.state = CharState::Stand;
    f.pos = Vector2::new(600.0, GROUND_Y);
    f.ground_plat = 0;
    let mut items = [Item::EMPTY; MAX_ITEMS];
    // just behind the heels, inside the box's half-width: the reach is symmetric front/back
    // (facing plays no part in the pickup geometry).
    items[0] = laser_gun_at(Vector2::new(600.0 - t.pickup_reach * 0.8, GROUND_Y));
    assert!(
        nearest_pickup(&f, &items, &t).is_some(),
        "an item behind the fighter within the box should be reachable"
    );
}

#[test]
fn pickup_fails_beyond_the_platform_height_tolerance() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut f = Fighter::spawn(600.0, 1.0);
    f.state = CharState::Stand;
    f.pos = Vector2::new(600.0, GROUND_Y);
    f.ground_plat = 0;
    let mut items = [Item::EMPTY; MAX_ITEMS];
    // PLATFORMS[3]: the top-center platform, 350px above the main floor -- past the pickup
    // box's vertical half-height.
    items[0] = laser_gun_at(Vector2::new(600.0, PLATFORMS[3].y));
    assert!(
        nearest_pickup(&f, &items, &t).is_none(),
        "an item far past the platform-height tolerance should stay unreachable"
    );
}

// ── catching a thrown item ───────────────────────────────────────────────────

/// P0 mid an active fighter-grab (CharState::Grab, frame inside the catch window) with a `kind`
/// item thrown by P1 arriving `dx` px in front of P0's hurtbox center (0 = dead-on).
/// `catcher_holding` seeds P0's hand slot (-1 = free) so the "already holding can't catch" case
/// can reuse this setup.
fn setup_thrown_arrival_at(t: &Tune, catcher_holding: i8, kind: ItemKind, dx: f32) -> SimState {
    let mut s = SimState::spawn();
    s.fighters[0].state = CharState::Grab;
    s.fighters[0].frame = t.grab_startup; // lands inside [grab_startup, grab_startup+grab_active)
    s.fighters[0].pos = Vector2::new(600.0, GROUND_Y);
    s.fighters[0].vel = Vector2::ZERO;
    s.fighters[0].ground_plat = 0;
    s.fighters[0].holding = catcher_holding;
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y); // the thrower, already let go of it
    if catcher_holding >= 0 {
        s.items[catcher_holding as usize] = Item {
            cell: None,
            owner: 0,
            ..laser_gun_at(Vector2::new(0.0, 0.0))
        };
    }
    let (bc, _) = hurtbox(&s.fighters[0]);
    s.items[0] = Item {
        cell: None,
        kind,
        thrown: true,
        owner: 1,
        vel: Vector2::ZERO, // parked at the test distance: one frame of gravity barely moves it
        ..laser_gun_at(bc + Vector2::new(dx, 0.0))
    };
    s
}

fn setup_thrown_arrival(t: &Tune, catcher_holding: i8) -> SimState {
    setup_thrown_arrival_at(t, catcher_holding, ItemKind::LaserGun, 0.0)
}

#[test]
fn thrown_item_is_caught_by_a_live_grab() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let s = setup_thrown_arrival(&t, -1);
    let c = step(&s, &[&IDLE, &IDLE], &t);
    assert_eq!(
        c.fighters[0].holding, 0,
        "the live grab caught it into the hand"
    );
    assert_eq!(c.items[0].owner, 0);
    assert!(
        !c.items[0].thrown,
        "caught -- no longer an armed projectile"
    );
    assert_eq!(
        c.fighters[0].damage, 0.0,
        "a catch replaces the hit, not adds to it"
    );
}

#[test]
fn a_full_hand_cannot_catch_and_takes_the_hit_instead() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let s = setup_thrown_arrival(&t, 1); // P0 already holding the item parked in slot 1
    let c = step(&s, &[&IDLE, &IDLE], &t);
    assert_eq!(
        c.fighters[0].holding, 1,
        "still holding the original item -- no catch happened"
    );
    assert!(
        c.fighters[0].damage > 0.0,
        "no free hand: the thrown item damages like today, catch or no catch"
    );
    assert!(
        !c.items[0].active(),
        "spent on impact, same as an uncaught throw hit"
    );
}

#[test]
fn a_graze_damages_but_only_a_near_center_arrival_is_caught() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    // the ring between the catch circle and the throw-hit circle: hurtbox radius (48 for a
    // grounded Grab pose) + CATCH_R_SCALE * hit.r (0.6 * 34 = 20.4) < 75 <= 48 + 34.
    let s = setup_thrown_arrival_at(&t, -1, ItemKind::LaserGun, 75.0);
    let c = step(&s, &[&IDLE, &IDLE], &t);
    assert_eq!(
        c.fighters[0].holding, -1,
        "a graze inside the hit circle but outside the tighter catch circle is NOT caught"
    );
    assert!(
        c.fighters[0].damage > 0.0,
        "the graze still damages like any throw hit"
    );
    assert!(!c.items[0].active(), "spent on impact");
    // same setup, arrival inside the catch circle: caught, no damage.
    let s = setup_thrown_arrival_at(&t, -1, ItemKind::LaserGun, 60.0);
    let c = step(&s, &[&IDLE, &IDLE], &t);
    assert_eq!(
        c.fighters[0].holding, 0,
        "inside the catch circle the same throw is caught"
    );
    assert_eq!(c.fighters[0].damage, 0.0);
}

#[test]
fn an_uncatchable_kind_hits_through_a_live_grab() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    // TetrisGun ships catchable=false (its ItemSpec row): the same live grab that snatches a
    // Pen eats the tetromino gun instead -- the per-kind gate, provable with one flipped kind.
    let s = setup_thrown_arrival_at(&t, -1, ItemKind::TetrisGun, 0.0);
    let c = step(&s, &[&IDLE, &IDLE], &t);
    assert_eq!(
        c.fighters[0].holding, -1,
        "an uncatchable kind is never caught, grab or no grab"
    );
    assert!(
        c.fighters[0].damage > 0.0,
        "it hits like any armed throw instead"
    );
    // the identical setup with a catchable kind (Pen) IS caught.
    let s = setup_thrown_arrival_at(&t, -1, ItemKind::Pen, 0.0);
    let c = step(&s, &[&IDLE, &IDLE], &t);
    assert_eq!(
        c.fighters[0].holding, 0,
        "the same grab catches a catchable kind (Pen)"
    );
    assert_eq!(c.fighters[0].damage, 0.0);
}

// ── air catch: the airborne GRAB press latches a catch window ────────────────

/// P0 hovering mid-air, empty-handed, far from any pickup; the thrown item is inserted at
/// contact by the test at the moment it wants "arrival". Hover is maintained by re-zeroing
/// pos/vel between steps (tests mutate state freely) so gravity can't land P0 mid-scenario.
fn hover(s: &mut SimState) {
    s.fighters[0].state = CharState::Air;
    s.fighters[0].pos = Vector2::new(600.0, GROUND_Y - 300.0);
    s.fighters[0].vel = Vector2::ZERO;
    s.fighters[0].ground_plat = -1;
}

/// The thrown item placed right on P0's hurtbox, mid-flight from P1.
fn arrive_at_p0(s: &mut SimState) {
    let (bc, _) = hurtbox(&s.fighters[0]);
    s.items[0] = Item {
        cell: None,
        thrown: true,
        owner: 1,
        vel: Vector2::new(200.0, 0.0),
        ..laser_gun_at(bc)
    };
}

#[test]
fn airborne_grab_press_catches_a_thrown_item_within_the_window() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    hover(&mut s);
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y);
    // press GRAB with nothing in reach: arms the catch latch, picks nothing up
    let grab = InputFrame { grab: true, ..IDLE };
    let mut c = step(&s, &[&grab, &IDLE], &t);
    assert_eq!(c.fighters[0].holding, -1, "nothing in reach at the press");
    // three idle frames later (still inside the window) the thrown item arrives
    for _ in 0..3 {
        hover(&mut c);
        c = step(&c, &[&IDLE, &IDLE], &t);
    }
    hover(&mut c);
    arrive_at_p0(&mut c);
    c = step(&c, &[&IDLE, &IDLE], &t);
    assert_eq!(
        c.fighters[0].holding, 0,
        "the latched air grab caught the arriving item"
    );
    assert_eq!(c.items[0].owner, 0);
    assert!(!c.items[0].thrown, "caught -- disarmed into a held item");
    assert_eq!(c.fighters[0].damage, 0.0, "a catch replaces the hit");
    assert_eq!(
        c.fighters[0].catch_win, 0,
        "the latch is spent by the catch -- one press can't catch twice"
    );
}

#[test]
fn a_stale_airborne_grab_press_does_not_catch() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    hover(&mut s);
    s.fighters[1].pos = Vector2::new(900.0, GROUND_Y);
    let grab = InputFrame { grab: true, ..IDLE };
    let mut c = step(&s, &[&grab, &IDLE], &t);
    // idle past the whole window before the item shows up
    for _ in 0..12 {
        hover(&mut c);
        c = step(&c, &[&IDLE, &IDLE], &t);
    }
    assert_eq!(c.fighters[0].catch_win, 0, "the latch expired");
    hover(&mut c);
    arrive_at_p0(&mut c);
    c = step(&c, &[&IDLE, &IDLE], &t);
    assert_eq!(
        c.fighters[0].holding, -1,
        "no live latch: the press was too early, nothing caught"
    );
    assert!(
        c.fighters[0].damage > 0.0,
        "the thrown item hits like today when the window is gone"
    );
    assert!(!c.items[0].active(), "spent on impact");
}

// ── grab-hold: a stick deflection alone picks + fires the throw ─────────────
// (item 5, ac-ship-backlog.md: direction alone selects fthrow/bthrow/uthrow/dthrow -- no
// direction+attack chord, no grab re-press. `resolve_grab`'s hold-maintenance arm.)

/// P0 already holding P1 (`GrabHold`/`Grabbed`, linked), `hold_frame` frames into the hold --
/// the state `resolve_grab`'s catch arm leaves behind, built directly so each test can place
/// `g.frame` on either side of the grace without stepping through the catch window first.
fn holding_pair(t: &Tune, hold_frame: i64) -> SimState {
    let mut s = SimState::spawn();
    s.fighters[0].state = CharState::GrabHold;
    s.fighters[0].frame = hold_frame;
    s.fighters[0].pos = Vector2::new(600.0, GROUND_Y);
    s.fighters[0].ground_plat = 0;
    s.fighters[0].facing = 1.0;
    s.fighters[0].grab_link = 1;
    s.fighters[0].grab_timer = t.grab_hold;
    s.fighters[1].state = CharState::Grabbed;
    s.fighters[1].pos = Vector2::new(600.0 + GRAB_HELD_X, GROUND_Y);
    s.fighters[1].ground_plat = 0;
    s.fighters[1].facing = -1.0;
    s.fighters[1].grab_link = 0;
    s.fighters[1].grab_timer = t.grab_hold;
    s
}

#[test]
fn forward_stick_past_the_grace_fires_fthrow() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    // seed at the grace boundary: `reduce_next_state` ticks `frame` to GRAB_HOLD_GRACE + 1
    // before `resolve_grab` reads it this step, landing just past the grace.
    let s = holding_pair(&t, GRAB_HOLD_GRACE);
    let fwd = InputFrame {
        dir: 1.0, // same side as P0's facing (1.0) -> forward
        ..IDLE
    };
    let c = step(&s, &[&fwd, &IDLE], &t);
    assert_eq!(c.fighters[0].grab_link, -1, "grabber released on the throw");
    assert_eq!(c.fighters[1].grab_link, -1, "victim released on the throw");
    assert!(c.fighters[1].hitstun > 0, "victim launched with hitstun");
    assert_eq!(
        c.fighters[1].damage,
        ThrowData::FWD.damage,
        "fthrow's damage, not bthrow/uthrow/dthrow"
    );
    assert!(
        c.fighters[1].vel.x > 0.0,
        "fthrow fires the way the grabber faces"
    );
}

#[test]
fn back_stick_past_the_grace_fires_bthrow() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let s = holding_pair(&t, GRAB_HOLD_GRACE);
    let back = InputFrame {
        dir: -1.0, // opposite P0's facing (1.0) -> back
        ..IDLE
    };
    let c = step(&s, &[&back, &IDLE], &t);
    assert_eq!(c.fighters[0].grab_link, -1, "grabber released on the throw");
    assert_eq!(
        c.fighters[1].damage,
        ThrowData::BACK.damage,
        "bthrow's damage"
    );
    assert!(
        c.fighters[1].vel.x < 0.0,
        "bthrow fires behind the grabber, opposite fthrow"
    );
}

#[test]
fn up_stick_past_the_grace_fires_uthrow() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let s = holding_pair(&t, GRAB_HOLD_GRACE);
    let up = InputFrame {
        aim_y: -1.0, // up
        ..IDLE
    };
    let c = step(&s, &[&up, &IDLE], &t);
    assert_eq!(c.fighters[0].grab_link, -1, "grabber released on the throw");
    assert_eq!(
        c.fighters[1].damage,
        ThrowData::UP.damage,
        "uthrow's damage"
    );
    assert!(c.fighters[1].vel.y < 0.0, "uthrow launches upward");
}

#[test]
fn a_stick_carried_in_from_the_run_in_does_not_instant_throw() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    // fresh hold (frame 0, exactly what the catch arm leaves behind) with the stick still
    // hard over from running in for the grab -- must NOT read as a throw pick until the
    // grace elapses.
    let mut s = holding_pair(&t, 0);
    let held_over = InputFrame { dir: 1.0, ..IDLE };
    for n in 0..GRAB_HOLD_GRACE {
        s = step(&s, &[&held_over, &IDLE], &t);
        assert_eq!(
            s.fighters[0].state,
            CharState::GrabHold,
            "frame {n}: still held inside the grace despite the carried-in stick"
        );
        assert_eq!(s.fighters[0].grab_link, 1, "frame {n}: no throw fired yet");
    }
    // one frame later the grace has elapsed; the still-held stick now fires the throw.
    s = step(&s, &[&held_over, &IDLE], &t);
    assert_eq!(
        s.fighters[0].grab_link, -1,
        "past the grace the same deflection throws"
    );
}

#[test]
fn pummel_deals_damage_without_throwing_then_shield_releases() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = holding_pair(&t, GRAB_HOLD_GRACE);
    let pummel = InputFrame {
        attack: true,
        ..IDLE
    };
    let before = s.fighters[1].damage;
    s = step(&s, &[&pummel, &IDLE], &t);
    assert!(s.fighters[1].damage > before, "pummel deals damage");
    assert_eq!(
        s.fighters[0].state,
        CharState::GrabHold,
        "attack alone pummels -- it does not throw"
    );
    assert_eq!(s.fighters[0].grab_link, 1, "still linked after the pummel");

    let shield = InputFrame {
        shield_pressed: true,
        ..IDLE
    };
    s = step(&s, &[&shield, &IDLE], &t);
    assert_eq!(s.fighters[0].state, CharState::Stand, "shield lets go");
    assert_eq!(s.fighters[0].grab_link, -1, "grabber released");
    // Released AT THE SLAVE POS, off its feet: Air is the honest state (the ground refs were
    // cleared at the catch; standing re-derives on the landing sweep a frame later).
    assert_eq!(
        s.fighters[1].state,
        CharState::Air,
        "victim released too (airborne, re-lands)"
    );
    assert_eq!(s.fighters[1].grab_link, -1);
    // and it lands back on its feet within a few frames instead of floating.
    for _ in 0..30 {
        s = step(&s, &[&IDLE, &IDLE], &t);
    }
    assert!(
        !airborne(s.fighters[1].state),
        "released victim re-lands, state={:?}",
        s.fighters[1].state
    );
}
