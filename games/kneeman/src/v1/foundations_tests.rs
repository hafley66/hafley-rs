// Smash-foundations v1: shield block, smash charge, ledge options, getup attack, clank,
// walljump, footstool, crawl. Declared as a crate-root child so `super::*` is the crate root.

use super::*;

/// Two grounded fighters facing each other, `gap` px apart, p0 on the left.
fn duo(gap: f32) -> (SimState, Tune) {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    for (k, (x, face)) in [(600.0_f32, 1.0_f32), (600.0 + gap, -1.0_f32)]
        .into_iter()
        .enumerate()
    {
        let f = &mut s.fighters[k];
        f.state = CharState::Stand;
        f.ground_plat = 0;
        f.pos = Vector2::new(x, GROUND_Y);
        f.facing = face;
    }
    (s, t)
}

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

// ── shield block ────────────────────────────────────────────────────────────

#[test]
fn shield_blocks_a_jab() {
    let (s, t) = duo(60.0);
    let atk = InputFrame {
        attack: true,
        ..IDLE
    };
    let guard = InputFrame {
        shield_held: true,
        shield_pressed: true,
        ..IDLE
    };
    let mut c = step(&s, &[&atk, &guard], &t);
    assert_eq!(c.fighters[1].state, CharState::Shield, "guard goes up");
    let hp0 = c.fighters[1].shield_hp;
    for _ in 0..8 {
        c = step(&c, &[&IDLE, &guard], &t);
    }
    let b = c.fighters[1];
    assert_eq!(b.damage, 0.0, "a blocked hit deals no percent");
    assert_eq!(b.hitstun, 0, "no launch through the shield");
    assert_eq!(b.state, CharState::Shield, "still guarding");
    assert!(
        b.shield_hp < hp0 - t.jab.boxes[0].damage,
        "shield hp ate the hit (plus hold decay): {} vs {hp0}",
        b.shield_hp
    );
    assert!(
        c.fighters[0].damage == 0.0 && c.fighters[0].hitstun == 0,
        "the attacker is untouched, just blocked"
    );
}

#[test]
fn empty_shield_breaks_into_dizzy_then_restores() {
    let (mut s, t) = duo(60.0);
    // a single-box heavy on the way in (a jab's LATER windows would legally hit the
    // freshly-broken shield — one box keeps the break itself the thing under test)
    s.fighters[0].state = CharState::Fsmash;
    s.fighters[0].arm_hits();
    let guard = InputFrame {
        shield_held: true,
        shield_pressed: true,
        ..IDLE
    };
    let mut c = step(&s, &[&IDLE, &guard], &t);
    // low enough that the 17% smash breaks it, high enough that hold decay (0.2/f) doesn't
    // beat the box to it during the wind-up
    c.fighters[1].shield_hp = 10.0;
    for _ in 0..(t.fsmash.total() + 2) {
        c = step(&c, &[&IDLE, &guard], &t);
    }
    assert_eq!(
        c.fighters[1].state,
        CharState::ShieldBreak,
        "the smash shattered the sliver of shield"
    );
    assert_eq!(
        c.fighters[1].damage, 0.0,
        "the breaking hit was still blocked"
    );
    // dizzy runs shieldbreak_frames of ADVANCING frames; hitlag froze a few, so pad past it.
    for _ in 0..(t.shieldbreak_frames + 40) {
        c = step(&c, &[&IDLE, &IDLE], &t);
    }
    let b = c.fighters[1];
    assert_eq!(b.state, CharState::Stand, "dizzy ends on its own");
    assert_eq!(b.shield_hp, t.shield_max, "shield comes back whole");
}

// ── smash charge ────────────────────────────────────────────────────────────

/// Run an fsmash with `hold` frames of charge and report the victim's damage after the hit.
fn fsmash_payout(hold: i64, t: &Tune) -> f32 {
    let (mut s, _) = duo(60.0);
    let f = &mut s.fighters[0];
    f.state = CharState::Fsmash;
    f.arm_hits();
    f.charge = 0;
    let held = InputFrame {
        attack_held: true,
        ..IDLE
    };
    let mut c = s;
    for _ in 0..hold {
        c = step(&c, &[&held, &IDLE], &t);
        assert_eq!(
            c.fighters[0].state,
            CharState::Fsmash,
            "pin holds the state"
        );
        assert_eq!(c.fighters[0].frame, 0, "pinned at frame 0 while held");
    }
    for _ in 0..(t.fsmash.total() + 2) {
        c = step(&c, &[&IDLE, &IDLE], &t);
    }
    c.fighters[1].damage
}

#[test]
fn charged_smash_banks_frames_and_hits_harder() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let plain = fsmash_payout(0, &t);
    let charged = fsmash_payout(30, &t);
    assert!(plain > 0.0, "the uncharged swing connects");
    let expect = plain * (1.0 + 0.5 * (t.charge_dmg - 1.0)); // 30/60 of the way to full
    assert!(
        (charged - expect).abs() < 0.3,
        "half charge pays ~{expect}, got {charged}"
    );
}

#[test]
fn charge_caps_and_releases_on_its_own() {
    let (mut s, t) = duo(400.0); // far apart: nobody gets hit, we just watch the clock
    let f = &mut s.fighters[0];
    f.state = CharState::Fsmash;
    f.arm_hits();
    let held = InputFrame {
        attack_held: true,
        ..IDLE
    };
    let mut c = s;
    for _ in 0..(t.charge_max + 10) {
        c = step(&c, &[&held, &IDLE], &t);
    }
    assert_eq!(c.fighters[0].charge, t.charge_max, "the bank caps");
    assert!(
        c.fighters[0].frame > 0,
        "at the cap the swing comes out even though the button is still down"
    );
}

// ── clank / priority ────────────────────────────────────────────────────────

/// Both fighters mid-jab, boxes about to overlap between them.
fn clashing_jabs() -> (SimState, Tune) {
    let (mut s, t) = duo(88.0); // jab off.x 44 each: the boxes meet in the middle
    for f in &mut s.fighters[..2] {
        f.state = CharState::Jab;
        f.frame = t.jab.boxes[0].start - 1; // live next step
        f.arm_hits();
    }
    (s, t)
}

#[test]
fn even_trade_rebounds_both() {
    let (s, t) = clashing_jabs();
    let c = step(&s, &[&IDLE, &IDLE], &t);
    assert_eq!(c.fighters[0].state, CharState::Rebound, "p0 clanked");
    assert_eq!(c.fighters[1].state, CharState::Rebound, "p1 clanked");
    assert_eq!(
        c.fighters[0].damage + c.fighters[1].damage,
        0.0,
        "no hit paid out"
    );
    assert!(
        c.fighters[0].vel.x < 0.0 && c.fighters[1].vel.x > 0.0,
        "shoved apart"
    );
    let mut c = c;
    for _ in 0..(t.rebound_frames + 1) {
        c = step(&c, &[&IDLE, &IDLE], &t);
    }
    assert_eq!(
        c.fighters[0].state,
        CharState::Stand,
        "stagger ends in neutral"
    );
}

#[test]
fn stronger_move_wins_the_clank() {
    let (mut s, t) = duo(100.0);
    // p0's fsmash (17%) meets p1's jab launcher (6%): gap > clank_diff, jab alone cancels.
    s.fighters[0].state = CharState::Fsmash;
    s.fighters[0].frame = t.fsmash.boxes[0].start - 1;
    s.fighters[0].arm_hits();
    s.fighters[1].state = CharState::Jab;
    s.fighters[1].frame = t.jab.boxes[2].start - 1;
    s.fighters[1].arm_hits();
    let c = step(&s, &[&IDLE, &IDLE], &t);
    assert_eq!(
        c.fighters[0].state,
        CharState::Fsmash,
        "the heavy swing powers through"
    );
    assert_eq!(c.fighters[0].damage, 0.0, "the cancelled jab never lands");
    assert!(
        c.fighters[1].hitstun > 0,
        "the out-prioritized fighter eats the smash on the same frame"
    );
}

// ── walljump ────────────────────────────────────────────────────────────────

#[test]
fn walljump_kicks_off_the_stage_face() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::Air;
    f.ground_plat = -1;
    // outside the right face, below the ledge-snap y-window, drifting into the wall
    f.pos = Vector2::new(FLOOR_RIGHT + ECB_HALF_W + 6.0, GROUND_Y + 150.0);
    f.vel = Vector2::new(-400.0, 60.0);
    let into = InputFrame { dir: -1.0, ..IDLE };
    let mut c = step(&s, &[&into, &IDLE], &t);
    assert!(c.fighters[0].wall_touch > 0, "wall contact arms the window");
    // stick DEFLECTED (away) on the jump frame: this is the kick path. A neutral-stick
    // jump is the quiet hop now (see neutral_jump_from_wall_hops_straight_up).
    let jump = InputFrame {
        jump: true,
        jump_held: true,
        dir: 1.0,
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
    assert!(f.vel.x > 0.0, "kicked away from the wall (rightward)");
    assert_eq!(f.air_jumps, 1, "no air jump spent");
    assert_eq!(f.wall_touch, 0, "the window is spent");
}

#[test]
fn neutral_jump_from_wall_hops_straight_up() {
    // let go of the stick, THEN jump: no kick, no sharp turn -- a straight-up hop along
    // the wall (2026-07-04: walljump was too touchy jumping out of a cling from neutral).
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::Air;
    f.ground_plat = -1;
    f.pos = Vector2::new(FLOOR_RIGHT + ECB_HALF_W + 6.0, GROUND_Y + 150.0);
    f.vel = Vector2::new(-400.0, 60.0);
    let facing_before = f.facing;
    let into = InputFrame { dir: -1.0, ..IDLE };
    let mut c = step(&s, &[&into, &IDLE], &t);
    assert!(c.fighters[0].wall_touch > 0, "wall contact arms the window");
    let jump = InputFrame {
        jump: true,
        jump_held: true,
        ..IDLE // stick at NEUTRAL
    };
    c = step(&c, &[&jump, &IDLE], &t);
    let f = c.fighters[0];
    assert_eq!(f.vel.x, 0.0, "no horizontal kick from a neutral-stick jump");
    assert!(
        (f.vel.y - (t.walljump_v + t.gravity * DT)).abs() < 1.0,
        "vertical hop, got {}",
        f.vel.y
    );
    assert_eq!(
        f.facing, facing_before,
        "no face-away turn on the neutral hop"
    );
    assert_eq!(f.air_jumps, 1, "no air jump spent");
    assert_eq!(f.wall_touch, 0, "the window is spent either way");
}

// ── buttonless walljump + wall cling (queue-2026-07-03 item 1) ──────────────

#[test]
fn buttonless_walljump_fires_on_deflection_away_no_jump_needed() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::Air;
    f.ground_plat = -1;
    // same rig as walljump_kicks_off_the_stage_face, but the second frame deflects AWAY from
    // the wall instead of pressing jump -- no button at all.
    f.pos = Vector2::new(FLOOR_RIGHT + ECB_HALF_W + 6.0, GROUND_Y + 150.0);
    f.vel = Vector2::new(-400.0, 60.0);
    let into = InputFrame { dir: -1.0, ..IDLE };
    let mut c = step(&s, &[&into, &IDLE], &t);
    assert!(c.fighters[0].wall_touch > 0, "wall contact arms the window");
    let away = InputFrame { dir: 1.0, ..IDLE }; // no jump/jump_held at all
    c = step(&c, &[&away, &IDLE], &t);
    let f = c.fighters[0];
    assert!(
        (f.vel.y - (t.walljump_v + t.gravity * DT)).abs() < 1.0,
        "buttonless vertical kick, got {}",
        f.vel.y
    );
    assert!(
        f.vel.x > 0.0,
        "kicked away from the wall (rightward) with no jump press"
    );
    assert_eq!(f.wall_touch, 0, "the window is spent");
}

#[test]
fn wall_cling_holds_y_then_releases_when_budget_expires() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::Air;
    f.ground_plat = -1;
    // same y-band as walljump_kicks_off_the_stage_face: below the ledge-snap window, still
    // inside the wall's span.
    f.pos = Vector2::new(FLOOR_RIGHT + ECB_HALF_W + 6.0, GROUND_Y + 150.0);
    f.vel = Vector2::new(-400.0, 0.0);
    let into = InputFrame { dir: -1.0, ..IDLE }; // holds TOWARD the wall the whole test: cling
    let mut c = step(&s, &[&into, &IDLE], &t);
    assert!(c.fighters[0].wall_touch > 0, "wall contact arms the window");
    let y0 = c.fighters[0].pos.y;
    for _ in 0..t.cling_frames {
        c = step(&c, &[&into, &IDLE], &t);
        assert!(
            (c.fighters[0].pos.y - y0).abs() < 1e-3,
            "clinging holds y in place, drifted to {}",
            c.fighters[0].pos.y
        );
    }
    assert_eq!(
        c.fighters[0].cling_used, t.cling_frames,
        "budget fully spent"
    );
    let y_end = c.fighters[0].pos.y;
    c = step(&c, &[&into, &IDLE], &t);
    assert!(
        c.fighters[0].pos.y > y_end,
        "cling budget exhausted: falling resumed even though still held into the wall"
    );
}

#[test]
fn wall_cling_budget_resets_on_landing() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::Air;
    f.ground_plat = -1;
    f.pos = Vector2::new(600.0, GROUND_Y - 5.0);
    f.vel = Vector2::new(0.0, 300.0); // falling straight down onto the main floor
    f.cling_used = 47; // pretend a wall cling spent part of this airtime's budget
    let mut c = s;
    let mut landed = false;
    for _ in 0..10 {
        c = step(&c, &[&IDLE, &IDLE], &t);
        if !airborne(c.fighters[0].state) && c.fighters[0].ground_plat >= 0 {
            landed = true;
            break;
        }
    }
    assert!(landed, "never landed");
    assert_eq!(
        c.fighters[0].cling_used, 0,
        "landing refreshes the cling budget"
    );
}

// ── footstool ───────────────────────────────────────────────────────────────

#[test]
fn footstool_pops_the_jumper_and_staggers_the_victim() {
    let (mut s, t) = duo(0.0);
    let f = &mut s.fighters[0];
    f.state = CharState::Air;
    f.ground_plat = -1;
    f.pos = Vector2::new(600.0, GROUND_Y - 130.0); // feet just over p1's head crown
    f.vel = Vector2::ZERO;
    let jump = InputFrame {
        jump: true,
        jump_held: true,
        ..IDLE
    };
    let c = step(&s, &[&jump, &IDLE], &t);
    let (a, v) = (c.fighters[0], c.fighters[1]);
    assert_eq!(a.vel.y, t.footstool_v, "hopped off the head");
    assert_eq!(a.air_jumps, 1, "the double jump was not consumed");
    assert!(v.hitstun > 0, "the victim flinches");
    assert_eq!(v.damage, 0.0, "a footstool deals no percent");
}

// ── crawl ───────────────────────────────────────────────────────────────────

#[test]
fn crawl_creeps_while_crouched() {
    let (s, t) = duo(400.0);
    let creep = InputFrame {
        down: true,
        dir: 1.0,
        ..IDLE
    };
    let mut c = step(&s, &[&creep, &IDLE], &t); // Stand -> Crouch
    c = step(&c, &[&creep, &IDLE], &t); // Crouch -> Crawl
    assert_eq!(c.fighters[0].state, CharState::Crawl);
    let x0 = c.fighters[0].pos.x;
    for _ in 0..20 {
        c = step(&c, &[&creep, &IDLE], &t);
    }
    let f = c.fighters[0];
    assert_eq!(f.state, CharState::Crawl, "keeps creeping while held");
    assert!(f.pos.x > x0, "actually moves");
    assert!(
        f.vel.x <= t.crawl_speed + 1.0,
        "capped at crawl speed, not walk"
    );
    let (cc, cr) = hurtbox(&f);
    let mut standing = f;
    standing.state = CharState::Stand;
    let (sc, sr) = hurtbox(&standing);
    assert!(cc.y > sc.y && cr < sr, "crawling keeps the crouch profile");
    // release down: back to neutral
    c = step(&c, &[&IDLE, &IDLE], &t);
    assert_eq!(c.fighters[0].state, CharState::Stand);
}

// ── getup attack ────────────────────────────────────────────────────────────

#[test]
fn knockdown_attack_is_the_rising_sweep() {
    let (mut s, t) = duo(80.0);
    let f = &mut s.fighters[0];
    f.state = CharState::Knockdown;
    f.frame = 20; // past KNOCKDOWN_LOCK, before auto-getup
    let atk = InputFrame {
        attack: true,
        ..IDLE
    };
    let mut c = step(&s, &[&atk, &IDLE], &t);
    assert_eq!(c.fighters[0].state, CharState::GetupAttack);
    assert!(
        c.fighters[0].intangible,
        "safe through the rise (startup i-frames)"
    );
    for _ in 0..(t.getup_attack.total() + 20) {
        // +20: hitlag on the connect freezes the swing's clock for a few frames
        c = step(&c, &[&IDLE, &IDLE], &t);
    }
    assert!(
        c.fighters[1].damage > 0.0,
        "the front window caught the standing opponent"
    );
    assert_eq!(c.fighters[0].state, CharState::Stand, "ends in neutral");
}

// ── ledge options ───────────────────────────────────────────────────────────

/// A fighter hanging on the right ledge (facing the stage).
fn hanging() -> (SimState, Tune) {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::LedgeHold;
    f.pos = Vector2::new(FLOOR_RIGHT, GROUND_Y + 44.0);
    f.vel = Vector2::ZERO;
    f.facing = -1.0;
    f.frame = 40; // ledge i-frames spent
    (s, t)
}

#[test]
fn ledge_attack_climbs_and_swings() {
    let (s, t) = hanging();
    let atk = InputFrame {
        attack: true,
        ..IDLE
    };
    let mut c = step(&s, &[&atk, &IDLE], &t);
    let f = c.fighters[0];
    assert_eq!(f.state, CharState::LedgeAttack);
    assert_eq!(f.pos.x, FLOOR_RIGHT - 30.0, "planted just inside the lip");
    assert_eq!(f.ground_plat, 0, "on the stage, not hanging");
    assert!(f.intangible, "startup is safe");
    for _ in 0..(t.ledge_attack.total() + 2) {
        c = step(&c, &[&IDLE, &IDLE], &t);
    }
    assert_eq!(c.fighters[0].state, CharState::Stand);
}

#[test]
fn ledge_roll_comes_up_intangible_and_inward() {
    let (s, t) = hanging();
    let guard = InputFrame {
        shield_held: true,
        shield_pressed: true,
        ..IDLE
    };
    let mut c = step(&s, &[&guard, &IDLE], &t);
    let f = c.fighters[0];
    assert_eq!(f.state, CharState::LedgeRoll);
    assert!(f.intangible, "the whole roll is safe");
    assert!(f.vel.x < 0.0, "rolls inward (facing the stage)");
    let x0 = f.pos.x;
    for _ in 0..(t.techroll_frames + 1) {
        c = step(&c, &[&IDLE, &IDLE], &t);
    }
    let f = c.fighters[0];
    assert_eq!(f.state, CharState::Stand);
    assert!(f.pos.x < x0, "ended deeper onto the stage");
}

// ── badges (items that mod the character) ───────────────────────────────────

/// A wings badge resting at the fighter's feet.
fn badge_at_feet(s: &mut SimState, x: f32) {
    s.items[0] = Item {
        cell: None,
        kind: ItemKind::WingsBadge,
        pos: Vector2::new(x, GROUND_Y),
        vel: Vector2::ZERO,
        owner: -1,
        gas: 1.0,
        gas_max: 1.0,
        timer: 0,
        facing: 1.0,
        tool: ToolKind::TrailPen,
        stroke: 0,
        thrown: false,
        mount: -1,
        hp: 0.0,
    };
}

#[test]
fn badge_attaches_instead_of_being_held() {
    let (mut s, t) = duo(400.0);
    badge_at_feet(&mut s, 610.0);
    let atk = InputFrame {
        attack: true,
        ..IDLE
    };
    let c = step(&s, &[&atk, &IDLE], &t);
    let f = c.fighters[0];
    assert!(f.has_badge(Badge::Wings), "the pickup set the badge bit");
    assert_eq!(f.holding, -1, "no hand slot taken");
    assert!(!c.items[0].active(), "the item was consumed on the spot");
    // nothing to drop or throw: a grab press now is just a fighter-grab attempt
    let grab = InputFrame { grab: true, ..IDLE };
    let c = step(&c, &[&grab, &IDLE], &t);
    assert_eq!(
        c.fighters[0].state,
        CharState::Grab,
        "grab has no item to toss"
    );
}

#[test]
fn wings_badge_air_jumps_forever() {
    let (mut s, t) = duo(400.0);
    let f = &mut s.fighters[0];
    f.state = CharState::Air;
    f.ground_plat = -1;
    f.pos = Vector2::new(600.0, GROUND_Y - 400.0);
    f.badges = Badge::Wings as u8;
    f.air_jumps = 1;
    let jump = InputFrame {
        jump: true,
        jump_held: true,
        ..IDLE
    };
    let mut c = s;
    for round in 0..4 {
        c = step(&c, &[&jump, &IDLE], &t);
        let f = c.fighters[0];
        assert!(f.vel.y < 0.0, "jump #{round} popped upward");
        assert_eq!(f.air_jumps, 1, "jump #{round} spent nothing");
        for _ in 0..12 {
            c = step(&c, &[&IDLE, &IDLE], &t); // fall a beat between presses
        }
    }
}

// ── character tag (char_id as sim state) ────────────────────────────────────

#[test]
fn char_id_is_world_state_and_survives_respawn() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let s = SimState::spawn_n(4);
    assert_eq!(
        [0, 1, 0, 1],
        [
            s.fighters[0].char_id,
            s.fighters[1].char_id,
            s.fighters[2].char_id,
            s.fighters[3].char_id
        ],
        "default cast alternates"
    );
    let mut s = s;
    s.fighters[1].char_id = 3; // an in-map switch is just this mutation inside step
    s.fighters[1].state = CharState::Air;
    s.fighters[1].ground_plat = -1;
    s.fighters[1].pos = Vector2::new(-4000.0, GROUND_Y); // past the blast zone
    let c = step(&s, &[&IDLE, &IDLE], &t);
    assert_eq!(
        c.fighters[1].char_id, 3,
        "the character tag rides through a KO"
    );
}

#[test]
fn badges_clear_on_respawn() {
    let (mut s, t) = duo(400.0);
    s.fighters[0].badges = Badge::Wings as u8;
    s.fighters[0].damage = 80.0;
    s.fighters[0].state = CharState::Air;
    s.fighters[0].ground_plat = -1;
    s.fighters[0].pos = Vector2::new(-4000.0, GROUND_Y); // past the blast zone
    let c = step(&s, &[&IDLE, &IDLE], &t);
    let f = c.fighters[0];
    assert_eq!(f.damage, 0.0, "the stock reset");
    assert!(
        !f.has_badge(Badge::Wings),
        "badges are stock-scoped wear items -- a KO strips them all"
    );
}

// ── dash-stop (Skid) ─────────────────────────────────────────────────────────
// Pins the previously-untested Skid arm: releasing mid-dash/run brakes at dashstop_friction
// (grippier than ground_friction), the brake always runs velocity through 0 before a reversal is
// allowed to fire a fresh dash, and dash-dancing inside the dash window flips facing without ever
// touching Skid.

#[test]
fn dash_release_skids_to_a_monotonic_stop() {
    let (s, t) = duo(400.0);
    let mut c = s;
    c.fighters[0].state = CharState::Dash;
    c.fighters[0].facing = 1.0;
    c.fighters[0].vel = Vector2::new(t.run_speed, 0.0);

    // release the stick: Dash's `mag < WALK_THRESH` arm fires immediately, no dash_window wait.
    c = step(&c, &[&IDLE, &IDLE], &t);
    assert_eq!(
        c.fighters[0].state,
        CharState::Skid,
        "release mid-dash enters Skid"
    );
    assert_eq!(c.fighters[0].facing, 1.0, "no facing change on Skid entry");

    let mut prev = c.fighters[0].vel.x;
    assert!(prev > 0.0, "still carrying dash speed into the brake");
    let mut frames = 0;
    while c.fighters[0].state == CharState::Skid {
        c = step(&c, &[&IDLE, &IDLE], &t);
        let v = c.fighters[0].vel.x;
        assert!(
            v <= prev + 1e-3,
            "Skid must decelerate monotonically, never speed back up"
        );
        assert!(v >= 0.0, "neutral stick: brake must not run past 0");
        prev = v;
        frames += 1;
        assert!(frames < 60, "Skid never reached rest");
    }
    assert_eq!(
        c.fighters[0].state,
        CharState::Stand,
        "brake finishes in neutral Stand"
    );
    assert_eq!(c.fighters[0].vel.x, 0.0, "at rest");
    assert_eq!(
        c.fighters[0].facing, 1.0,
        "facing survives the whole brake untouched"
    );
}

#[test]
fn run_release_skids_the_same_way() {
    let (s, t) = duo(400.0);
    let mut c = s;
    c.fighters[0].state = CharState::Run;
    c.fighters[0].facing = -1.0;
    c.fighters[0].vel = Vector2::new(-t.run_speed, 0.0);

    c = step(&c, &[&IDLE, &IDLE], &t);
    assert_eq!(
        c.fighters[0].state,
        CharState::Skid,
        "release mid-run enters Skid"
    );

    let mut prev = c.fighters[0].vel.x;
    let mut frames = 0;
    while c.fighters[0].state == CharState::Skid {
        c = step(&c, &[&IDLE, &IDLE], &t);
        let v = c.fighters[0].vel.x;
        assert!(
            v >= prev - 1e-3,
            "monotonic toward 0 from the negative side"
        );
        assert!(v <= 0.0, "neutral stick: brake must not run past 0");
        prev = v;
        frames += 1;
        assert!(frames < 60, "Skid never reached rest");
    }
    assert_eq!(c.fighters[0].state, CharState::Stand);
    assert_eq!(c.fighters[0].vel.x, 0.0);
    assert_eq!(
        c.fighters[0].facing, -1.0,
        "no facing change from a run brake"
    );
}

// NOTE on the discrepancy this test surfaces: the doc comment on `CharState::Skid` reads like the
// brake settles AT 0 for an observable frame and only THEN launches the reversal dash. What the
// code actually does is fold both steps into the same tick: the frame whose *previous* stored
// velocity was already within one friction step of 0 is the same frame that clamps to 0.0 and,
// in that same match arm, immediately overwrites vel with the dash_init kick. So the recorded
// per-frame trajectory has exactly one frame where |delta| blows past the friction-step bound
// (prev -> -dash_init); there is no separate externally-visible frame holding vel == 0. The test
// below asserts the actual shape (bounded deltas for every frame except the kick frame, and the
// kick frame's *input* to that tick was already within one friction step of 0) rather than a
// literal every-frame delta bound.
#[test]
fn skid_reversal_crosses_zero_before_the_fresh_dash_kicks() {
    let (s, t) = duo(400.0);
    let mut c = s;
    c.fighters[0].state = CharState::Skid;
    c.fighters[0].facing = 1.0;
    c.fighters[0].vel = Vector2::new(t.run_speed, 0.0);
    let left = InputFrame { dir: -1.0, ..IDLE };

    let friction_step = t.dashstop_friction * DT;
    let mut prev = c.fighters[0].vel.x;
    let mut kicked = false;
    for _ in 0..60 {
        c = step(&c, &[&left, &IDLE], &t);
        let v = c.fighters[0].vel.x;
        if c.fighters[0].state == CharState::Dash {
            // the reversal tick: Skid clamped to 0 and launched the fresh opposite dash in the
            // SAME step (dash_init kick) -- see the discrepancy note above this test.
            assert_eq!(
                c.fighters[0].facing, -1.0,
                "fresh dash faces the held reverse direction"
            );
            assert!(
                (v - (-t.dash_init)).abs() < 1e-3,
                "fresh dash gets the full dash_init burst, not a partial one"
            );
            assert!(
                (0.0..=friction_step + 1e-3).contains(&prev),
                "the tick's incoming velocity ({prev}) must already be within one friction step \
                 of 0 -- the brake ran velocity through 0 before the kick, never a positive ->\
                 negative teleport from further out"
            );
            kicked = true;
            break;
        }
        assert_eq!(c.fighters[0].state, CharState::Skid, "still braking");
        let delta = v - prev;
        assert!(delta <= 1e-3, "still braking: velocity must not increase");
        assert!(
            delta >= -(friction_step + 1e-3),
            "per-frame delta bounded by the dashstop_friction step while still in Skid"
        );
        assert!(
            v >= -1e-3,
            "no sign flip while still braking -- Skid holds at/above 0 until the kick tick"
        );
        prev = v;
    }
    assert!(
        kicked,
        "held reverse past DASH_THRESH must eventually fire the opposite dash"
    );
}

#[test]
fn dash_dance_flips_facing_without_entering_skid() {
    let (s, t) = duo(400.0);
    let mut c = s;
    c.fighters[0].state = CharState::Dash;
    c.fighters[0].facing = 1.0;
    c.fighters[0].frame = 3; // well inside dash_window (12), no dash-to-run rollover yet
    c.fighters[0].vel = Vector2::new(t.dash_init, 0.0);
    let left = InputFrame { dir: -1.0, ..IDLE };

    let before = c.fighters[0].vel.x;
    c = step(&c, &[&left, &IDLE], &t);
    let f = c.fighters[0];
    assert_eq!(
        f.state,
        CharState::Dash,
        "dash-dance stays in Dash, never Skid"
    );
    assert_eq!(f.facing, -1.0, "facing flips to the new deflection");
    assert_eq!(f.vel.x, before, "no velocity teleport on the flip frame");
    assert_eq!(
        f.frame, 0,
        "force_reset restarts the dash window on the flip"
    );
}

#[test]
fn skid_brakes_harder_than_a_plain_ground_friction_coast() {
    let (s, t) = duo(400.0);

    let mut skid = s;
    skid.fighters[0].state = CharState::Skid;
    skid.fighters[0].vel = Vector2::new(t.run_speed, 0.0);
    let mut skid_frames = 0;
    while skid.fighters[0].vel.x != 0.0 {
        skid = step(&skid, &[&IDLE, &IDLE], &t);
        skid_frames += 1;
        assert!(skid_frames < 60, "Skid never reached rest");
    }

    let mut coast = s;
    coast.fighters[0].state = CharState::Stand;
    coast.fighters[0].vel = Vector2::new(t.run_speed, 0.0);
    let mut coast_frames = 0;
    while coast.fighters[0].vel.x != 0.0 {
        coast = step(&coast, &[&IDLE, &IDLE], &t);
        coast_frames += 1;
        assert!(
            coast_frames < 60,
            "ground_friction coast never reached rest"
        );
    }

    assert!(
        t.dashstop_friction > t.ground_friction,
        "tune sanity: dashstop_friction ({}) must exceed ground_friction ({})",
        t.dashstop_friction,
        t.ground_friction
    );
    assert!(
        skid_frames < coast_frames,
        "Skid ({skid_frames} frames) must stop faster than a plain ground_friction coast \
         ({coast_frames} frames) from the same starting speed"
    );
}
