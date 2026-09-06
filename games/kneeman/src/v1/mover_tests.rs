// Baked-fixture tests (stage/fixtures.rs): walljump on the ink pillar, the triangle-wave
// mover (purity/rollback, ride carry, landing inherit), and fixture invincibility.
// Declared as a crate-root child so `super::*` is the crate root.

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

// ── walljump on ink (the pillar) ────────────────────────────────────────────

#[test]
fn walljump_kicks_off_the_ink_pillar() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::Air;
    f.ground_plat = -1;
    // left of the pillar's wall face, ECB center inside its y-span, drifting into it
    // (mirrors foundations' walljump_kicks_off_the_stage_face, stage face → ink face)
    f.pos = Vector2::new(PILLAR_X - ECB_HALF_W - 6.0, PILLAR_TOP + 140.0);
    f.vel = Vector2::new(400.0, 60.0);
    let into = InputFrame { dir: 1.0, ..IDLE };
    let mut c = step(&s, &[&into, &IDLE], &t);
    assert!(
        c.fighters[0].wall_touch > 0,
        "ink wall contact arms the window"
    );
    // stick DEFLECTED (away, leftward) on the jump frame: the kick path. Neutral-stick
    // jumps hop straight up now (foundations_tests::neutral_jump_from_wall_hops_straight_up).
    let jump = InputFrame {
        jump: true,
        jump_held: true,
        dir: -1.0,
        ..IDLE
    };
    c = step(&c, &[&jump, &IDLE], &t);
    let f = c.fighters[0];
    // the same frame's gravity tick rides on top of the kick (identical to a double jump)
    assert!(
        (f.vel.y - (t.walljump_v + t.gravity * DT)).abs() < 1.0,
        "vertical kick, got {}",
        f.vel.y
    );
    assert!(f.vel.x < 0.0, "kicked away from the pillar (leftward)");
    assert_eq!(
        f.air_jumps, t.max_air_jumps as u8,
        "no air jump spent (wall touch refreshed them)"
    );
    assert_eq!(f.wall_touch, 0, "the window is spent");
}

#[test]
fn buttonless_walljump_fires_on_the_ink_pillar_no_jump_needed() {
    // ship-hull/ink variant of foundations_tests' buttonless pin: works on a drawn-ink Wall
    // surf, not just the stage face (queue-2026-07-03 item 1, generic wall layer).
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::Air;
    f.ground_plat = -1;
    f.pos = Vector2::new(PILLAR_X - ECB_HALF_W - 6.0, PILLAR_TOP + 140.0);
    f.vel = Vector2::new(400.0, 60.0);
    let into = InputFrame { dir: 1.0, ..IDLE };
    let mut c = step(&s, &[&into, &IDLE], &t);
    assert!(
        c.fighters[0].wall_touch > 0,
        "ink wall contact arms the window"
    );
    let away = InputFrame { dir: -1.0, ..IDLE }; // deflect away, no jump/jump_held at all
    c = step(&c, &[&away, &IDLE], &t);
    let f = c.fighters[0];
    assert!(
        (f.vel.y - (t.walljump_v + t.gravity * DT)).abs() < 1.0,
        "buttonless vertical kick off ink, got {}",
        f.vel.y
    );
    assert!(f.vel.x < 0.0, "kicked away from the pillar (leftward)");
    assert_eq!(f.wall_touch, 0, "the window is spent");
}

// ── mover determinism ───────────────────────────────────────────────────────

#[test]
fn mover_pos_is_pure_and_two_runs_from_one_snapshot_agree() {
    // purity: the triangle wave is periodic on the tick, exactly.
    for tk in [0u64, 7, 133, 179, 180, 359, 360, 12345] {
        assert_eq!(mover_pos(tk), mover_pos(tk + MOVER_PERIOD), "tick {tk}");
    }
    // stepping N frames twice from the same snapshot: identical mover pos and state bytes
    // (rollback re-simulation is exactly this — a re-run from an old snapshot).
    let t = Tune::from_char(&CharData::KNEEMAN);
    let s0 = SimState::spawn();
    let run = |mut s: SimState| {
        for _ in 0..100 {
            s = step(&s, &[&IDLE, &IDLE], &t);
        }
        s
    };
    let (a, b) = (run(s0), run(s0));
    assert_eq!(a.paths[MOVER_SLOT].pos, b.paths[MOVER_SLOT].pos);
    assert_eq!(
        a.paths[MOVER_SLOT].pos,
        mover_pos(a.tick),
        "stepped pos IS the pure function of the tick"
    );
    assert!(a == b, "full states identical");
    assert_eq!(
        bincode::serialize(&a).unwrap(),
        bincode::serialize(&b).unwrap(),
        "checksum bytes identical"
    );
}

// ── ride carry ──────────────────────────────────────────────────────────────

#[test]
fn idle_fighter_rides_the_mover_for_a_full_period() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    let home = mover_pos(0);
    let f = &mut s.fighters[0];
    f.state = CharState::Stand;
    f.ground_plat = 0; // the grounded-on-ink convention
    f.ground_ink = MOVER_SLOT as i8;
    f.pos = home;
    f.vel = Vector2::ZERO;
    let mut c = s;
    for _ in 0..(MOVER_PERIOD + 30) {
        c = step(&c, &[&IDLE, &IDLE], &t);
        let f = c.fighters[0];
        assert_eq!(
            f.ground_ink, MOVER_SLOT as i8,
            "still riding at tick {}",
            c.tick
        );
        assert!(
            (f.pos.x - c.paths[MOVER_SLOT].pos.x).abs() < 1.0,
            "x tracks the platform at tick {}: fighter {} vs mover {}",
            c.tick,
            f.pos.x,
            c.paths[MOVER_SLOT].pos.x
        );
        assert!((f.pos.y - MOVER_HOME.y).abs() < 0.01, "pinned to its face");
    }
}

// ── landing inherit ─────────────────────────────────────────────────────────

#[test]
fn landing_on_the_mover_inherits_its_velocity() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    // run the mover into its rightward half-period, then drop a fighter onto it
    for _ in 0..30 {
        s = step(&s, &[&IDLE, &IDLE], &t);
    }
    let m = s.paths[MOVER_SLOT].pos;
    let f = &mut s.fighters[0];
    f.state = CharState::Air;
    f.ground_plat = -1;
    f.pos = Vector2::new(m.x + 40.0, m.y - 40.0); // lead the platform a touch
    f.vel = Vector2::new(0.0, 300.0); // falling straight down
    let mut c = s;
    let mut landed = false;
    for _ in 0..30 {
        c = step(&c, &[&IDLE, &IDLE], &t);
        let f = c.fighters[0];
        if f.ground_ink == MOVER_SLOT as i8 {
            let sv = mover_step(c.tick).x * FPS; // the surf vel (px/s) this frame
            assert!(sv > 0.0, "phase check: mover still moving right");
            assert!(
                (f.vel.x - sv).abs() < 30.0,
                "landing frame vel.x includes the surf vel: {} vs {}",
                f.vel.x,
                sv
            );
            landed = true;
            break;
        }
    }
    assert!(landed, "never landed on the mover");
}

// ── invincibility ───────────────────────────────────────────────────────────

#[test]
fn fixtures_shrug_off_max_knockback_and_survive_prune() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    let (pillar0, mover0) = (s.paths[PILLAR_SLOT], s.paths[MOVER_SLOT]);
    // max-order strike on each fixture: contact right on the stroke, absurd damage
    let hit_pillar = strike_ink(
        &mut s.paths,
        Vector2::new(PILLAR_X, PILLAR_TOP + 100.0),
        40.0,
        &t.throw_item.hit,
        999.0,
        1.0,
        &s.nodes,
        &t,
    );
    let hit_mover = strike_ink(
        &mut s.paths,
        mover0.pos,
        40.0,
        &t.throw_item.hit,
        999.0,
        1.0,
        &s.nodes,
        &t,
    );
    assert!(
        !hit_pillar && !hit_mover,
        "baked fixtures don't even count as struck"
    );
    assert_eq!(
        s.paths[PILLAR_SLOT].pos, pillar0.pos,
        "pillar moved zero px"
    );
    assert_eq!(s.paths[MOVER_SLOT].pos, mover0.pos, "mover moved zero px");
    assert_eq!(s.paths[PILLAR_SLOT].vel, Vector2::ZERO);
    // contrast: the same strike launches a normal (density 1) player stroke
    let pts = [Vector2::new(560.0, 500.0), Vector2::new(660.0, 500.0)];
    s.paths[0] = rehydrate_stroke(&pts, 0, 0, &mut s.nodes, &mut s.free, &t);
    let at = s.paths[0].pos;
    assert!(strike_ink(
        &mut s.paths,
        at,
        40.0,
        &t.throw_item.hit,
        999.0,
        1.0,
        &s.nodes,
        &t
    ));
    assert!(s.paths[0].traveling(), "the normal stroke is launched");
    // prune: a still player stroke parked outside the blast frame dies; fixtures never do.
    let far = [Vector2::new(1900.0, 500.0), Vector2::new(2000.0, 500.0)];
    s.paths[1] = rehydrate_stroke(&far, 0, 0, &mut s.nodes, &mut s.free, &t);
    prune_outside(&mut s);
    assert!(!s.paths[1].active(), "out-of-zone player ink pruned");
    assert!(s.paths[PILLAR_SLOT].active(), "pillar exempt from prune");
    assert!(s.paths[MOVER_SLOT].active(), "mover exempt from prune");
}
