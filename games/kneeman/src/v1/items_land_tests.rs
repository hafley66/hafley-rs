// Free items join the generic floor solve (plans/body-unify.md step 3, items/floor.rs):
// end-to-end coverage through `step()` (not just the pure `resolve_floor_contact` unit tests in
// items/floor.rs itself). Declared as a crate-root child so `super::*` is the crate root (same
// convention as `grab_tests`/`tetris_drop_tests`).

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

/// An unowned ground item falling in from above `GROUND_Y`, the shape `maybe_spawn_item` /
/// `spawn_kind` already drop items in (queue-2026-07-03's "drop in from above" spawn).
fn falling_item(kind: ItemKind, above: f32) -> Item {
    Item {
        kind,
        pos: Vector2::new(600.0, GROUND_Y - above),
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
        hp: crate::v1::items::hurt::item_hp(kind),
    }
}

/// Step both fighters idle up to `max_frames` times, returning as soon as `done` reads true off
/// the slot; panics if it never does (mirrors `tetris_drop_tests`' settle-polling loop).
fn run_until(mut s: SimState, t: &Tune, max_frames: u32, done: impl Fn(&Item) -> bool) -> SimState {
    for _ in 0..max_frames {
        if done(&s.items[0]) {
            return s;
        }
        s = step(&s, &[&IDLE, &IDLE], t);
    }
    panic!("item never reached the expected resting state within {max_frames} frames");
}

// ── restitution-0 kinds: byte-identical dead-stop settle ─────────────────────────────────────

#[test]
fn restitution_zero_kind_settles_dead_stop_on_the_main_floor() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    s.fighters[0].pos = Vector2::new(-2000.0, GROUND_Y); // out of the item's way
    s.fighters[1].pos = Vector2::new(-2000.0, GROUND_Y);
    s.items[0] = falling_item(ItemKind::LaserGun, 300.0);

    let s = run_until(s, &t, 300, |it| {
        it.vel == Vector2::ZERO && it.pos.y >= GROUND_Y - 1.0
    });

    assert_eq!(
        s.items[0].pos.y, GROUND_Y,
        "rests exactly on the main floor, same as before"
    );
    assert_eq!(
        s.items[0].vel,
        Vector2::ZERO,
        "dead stop -- the old unconditional branch's output"
    );
    assert_eq!(s.items[0].owner, -1, "still an unowned ground pickup");
    // never overshoots past the floor even transiently: a restitution-0 row never bounces.
    assert!(s.items[0].pos.y >= GROUND_Y - 1e-3);
}

#[test]
fn bobgun_pickup_kind_also_settles_dead_stop() {
    // a second Land::Settle kind, to prove the material row (not a single hardcoded kind check)
    // is what drives the collapse -- every kind but the pen shares the same restitution-0 row.
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    s.fighters[0].pos = Vector2::new(-2000.0, GROUND_Y);
    s.fighters[1].pos = Vector2::new(-2000.0, GROUND_Y);
    s.items[0] = falling_item(ItemKind::BobGun, 300.0);

    let s = run_until(s, &t, 300, |it| {
        it.vel == Vector2::ZERO && it.pos.y >= GROUND_Y - 1.0
    });

    assert_eq!(s.items[0].pos.y, GROUND_Y);
    assert_eq!(s.items[0].vel, Vector2::ZERO);
}

// ── the pen's restitution>0 row: the one wired bounce ────────────────────────────────────────

#[test]
fn pen_bounces_off_the_floor_before_it_settles() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    s.fighters[0].pos = Vector2::new(-2000.0, GROUND_Y);
    s.fighters[1].pos = Vector2::new(-2000.0, GROUND_Y);
    // same drop height as the dead-stop tests (300px, below the top-center platform at x=600
    // so the only floor it ever meets is the main stage) -- terminal-ish approach speed by the
    // time it gets there is plenty to clear ITEM_LOCK_REBOUND at restitution 0.2.
    s.items[0] = falling_item(ItemKind::Pen, 300.0);

    // watch every frame until it first crosses the floor, and confirm it leaves the floor again
    // (a real bounce, not an instant lock) before it eventually comes back to rest there.
    let mut ever_bounced_back_up = false;
    let mut cur = s;
    for _ in 0..300 {
        let prev_y = cur.items[0].pos.y;
        cur = step(&cur, &[&IDLE, &IDLE], &t);
        if prev_y >= GROUND_Y - 1.0 && cur.items[0].pos.y < GROUND_Y - 1.0 {
            ever_bounced_back_up = true;
        }
        if cur.items[0].vel == Vector2::ZERO && cur.items[0].pos.y >= GROUND_Y - 1e-3 {
            break;
        }
    }
    assert!(
        ever_bounced_back_up,
        "the pen's restitution>0 row should visibly leave the floor at least once"
    );
    assert_eq!(
        cur.items[0].pos.y, GROUND_Y,
        "eventually comes to rest exactly on the floor"
    );
    assert_eq!(
        cur.items[0].vel,
        Vector2::ZERO,
        "and fully stops once its rebound decays"
    );
    assert_eq!(
        cur.items[0].owner, -1,
        "still an unowned ground pickup once settled"
    );
}

// ── grounded pickup-at-rest: the item's override keeps working, unchanged by the rewire ─────

#[test]
fn settled_ground_item_is_still_pickup_able_by_a_grounded_attack() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    s.fighters[0].pos = Vector2::new(-2000.0, GROUND_Y);
    s.fighters[1].pos = Vector2::new(-2000.0, GROUND_Y);
    s.items[0] = falling_item(ItemKind::LaserGun, 300.0);
    let mut s = run_until(s, &t, 300, |it| {
        it.vel == Vector2::ZERO && it.pos.y >= GROUND_Y - 1.0
    });

    // now bring a grounded, actionable fighter to stand on the settled item and press ATTACK.
    s.fighters[0].state = CharState::Stand;
    s.fighters[0].pos = Vector2::new(s.items[0].pos.x, GROUND_Y);
    s.fighters[0].ground_plat = 0;
    s.fighters[0].vel = Vector2::ZERO;
    let atk = InputFrame {
        attack: true,
        ..IDLE
    };
    let s = step(&s, &[&atk, &IDLE], &t);
    assert_eq!(
        s.fighters[0].holding, 0,
        "pickup-at-rest (the item's override, plans/body-unify.md layer 4) still fires"
    );
    assert_eq!(s.items[0].owner, 0);
}

// ── thrown-item disarm still settles the same way once it lands ─────────────────────────────

#[test]
fn a_thrown_settle_kind_that_lands_harmlessly_still_disarms_to_a_ground_item() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    // parked far from both fighters so it never grazes/gets-caught, only ever meets the floor.
    s.fighters[0].pos = Vector2::new(-2000.0, GROUND_Y);
    s.fighters[1].pos = Vector2::new(-2000.0, GROUND_Y);
    s.items[0] = Item {
        thrown: true,
        owner: 0, // the (absent-from-the-action) thrower; passes through them regardless
        vel: Vector2::new(0.0, -50.0),
        ..falling_item(ItemKind::LaserGun, 300.0)
    };

    let s = run_until(s, &t, 300, |it| !it.thrown);

    assert_eq!(
        s.items[0].owner, -1,
        "disarmed into a normal unowned ground item"
    );
    assert_eq!(s.items[0].vel, Vector2::ZERO);
    assert_eq!(s.items[0].pos.y, GROUND_Y);
}
