// Split out of lib.rs; declared as a crate-root child so `super::*` still means the crate root.

use super::*;

// Sex-kick nair: a strong early box (id 0) then a weaker, shallower tail box (id 1) at the same
// limb. Two windowed boxes, sequenced on the shared frame clock. One swing, two payoffs.
#[test]
fn nair_sex_kick_has_strong_early_weak_late() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let n = t.nair;
    assert_eq!(n.nbox, 2, "nair is a two-box sex kick");
    let early = &n.boxes[0];
    let late = &n.boxes[1];
    // the tail opens after the early window closes (sequenced, not overlapping).
    assert_eq!(
        late.start,
        early.start + early.len,
        "tail follows the early window"
    );
    assert!(early.damage > late.damage, "early hit must beat the tail");
    assert!(
        early.angle > late.angle,
        "tail launches shallower than the early pop"
    );
    // first box opens at startup; total spans both windows + recovery.
    let early_frame = n.startup;
    assert!(
        n.box_at(early_frame).is_some(),
        "a box is live on the first active frame"
    );
}

// A single-window attack reports the same box across its whole window (no tail to switch to).
#[test]
fn single_window_attack_is_constant() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let d = t.dash_attack;
    assert_eq!(d.nbox, 1, "dash attack is one window");
    let b0 = d.boxes[0];
    let a = d.box_at(b0.start).copied();
    let b = d.box_at(b0.start + b0.len - 1).copied();
    assert_eq!(a, b, "one box -> identical payoff every active frame");
    assert!(
        d.box_at(b0.start + b0.len).is_none(),
        "window closes after len frames"
    );
}

/// Two grounded fighters facing each other, p1 a hair in front of p0 (inside jab reach).
fn sparring() -> (SimState, Tune) {
    let t = Tune::from_char(&CharData::KNEEMAN);
    let mut s = SimState::spawn();
    for (k, (x, face)) in [(600.0_f32, 1.0_f32), (660.0_f32, -1.0_f32)]
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

// 3-punch jab: one attack press lands three sequenced hits (count the victim's damage jumps).
#[test]
fn jab_autocombo_lands_three_hits() {
    let (s, t) = sparring();
    let idle = InputFrame::default();
    let attack = InputFrame {
        attack: true,
        ..Default::default()
    };
    let mut c = step(&s, &[&attack, &idle], &t);
    assert_eq!(c.fighters[0].state, CharState::Jab, "press enters the jab");
    let mut hits = 0;
    let mut prev = c.fighters[1].damage;
    for _ in 0..(t.jab.total() + 4) {
        c = step(&c, &[&idle, &idle], &t);
        if c.fighters[1].damage > prev + 0.001 {
            hits += 1;
            prev = c.fighters[1].damage;
        }
    }
    assert_eq!(hits, 3, "the 3-punch jab connects three times");
}

// A hit forces the victim into Launched and zeroes its frame, cancelling a move it was mid-swing.
#[test]
fn hit_interrupts_into_launched() {
    let (mut s, t) = sparring();
    // p1 is mid-nair (its own hitbox window open) when p0's jab lands.
    s.fighters[1].state = CharState::Nair;
    s.fighters[1].frame = 6;
    s.fighters[1].ground_plat = -1;
    s.fighters[1].arm_hits();
    let idle = InputFrame::default();
    let attack = InputFrame {
        attack: true,
        ..Default::default()
    };
    let mut c = step(&s, &[&attack, &idle], &t);
    // run until the first jab box connects (within startup+a few frames).
    for _ in 0..8 {
        if c.fighters[1].state == CharState::Launched {
            break;
        }
        c = step(&c, &[&idle, &idle], &t);
    }
    assert_eq!(
        c.fighters[1].state,
        CharState::Launched,
        "the hit launches the victim"
    );
    assert_eq!(
        c.fighters[1].frame, 0,
        "launch resets the victim's state frame"
    );
    assert!(c.fighters[1].hitstun > 0, "victim is in hitstun");
}

// Lowest live id wins per victim: when two boxes overlap a target, the sweetspot (id 0) pays out.
#[test]
fn lowest_id_box_wins() {
    let t = Tune::from_char(&CharData::KNEEMAN);
    // craft a 2-box move where both boxes are live + overlapping on the same frame.
    let mut atk = AttackData::one(
        0,
        4,
        4,
        Hitbox {
            off: Vector2::new(40.0, -64.0),
            r: 40.0,
            damage: 12.0,
            angle: 90.0,
            bkb: 30.0,
            kbg: 60.0,
            ..Hitbox::NONE
        },
    );
    atk.boxes[1] = Hitbox {
        targets: crate::v1::HitTargets::Both,
        id: 1,
        start: 0,
        len: 4,
        off: Vector2::new(40.0, -64.0),
        r: 40.0,
        damage: 2.0,
        angle: 10.0,
        bkb: 4.0,
        kbg: 8.0,
        set_kb: 0.0,
        transcendent: false,
        refresh: 0,
    };
    atk.nbox = 2;
    // both windows contain frame 1; box_at must return the id-0 (12%) box, not the id-1 (2%) one.
    let chosen = atk.box_at(1).copied().unwrap();
    assert_eq!(chosen.id, 0, "lowest id wins");
    assert_eq!(
        chosen.damage, 12.0,
        "the sweetspot pays out, not the sourspot"
    );
    let _ = t;
}

// Per-state hurtbox: crouching pulls the circle lower and smaller than standing, so a high jab
// can sail over a duck. Knockdown is lower still.
#[test]
fn crouch_hurtbox_ducks_under_standing() {
    let mut f = Fighter::spawn(600.0, 1.0);
    f.state = CharState::Stand;
    let (sc, sr) = hurtbox(&f);
    f.state = CharState::Crouch;
    let (cc, cr) = hurtbox(&f);
    assert!(cc.y > sc.y, "crouch center sits lower (closer to the feet)");
    assert!(cr < sr, "crouch body shrinks");
    f.state = CharState::Knockdown;
    let (kc, _) = hurtbox(&f);
    assert!(kc.y > cc.y, "floored is lower than a crouch");
}
