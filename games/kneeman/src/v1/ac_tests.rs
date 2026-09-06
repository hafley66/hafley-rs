// The Armored Core overlay (plans/ac-overlay.md): touch-attach body-replace, boost movement, and
// the c-stick arm gun. Declared as a crate-root child so `super::*` is the crate root; the `ac`
// module (weapon data) rides in on that glob.

use super::*;
use crate::v1::ac::{ArmWeapon, arm_spec};

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

/// Attach the AC badge to a fighter in-place (skips the touch dance for tests about behavior AFTER
/// the transform).
fn make_ac(f: &mut Fighter, arm: u8) {
    f.badges |= Badge::AcCore as u8;
    f.arm = arm;
    f.arm_cd = 0;
    f.qb_cd = 0;
}

/// Fighter 0 grounded mid-stage; fighter 1 tucked far LEFT so a right-firing arm gun never grazes
/// it (bolt hits despawn the projectile, which would foul the shot counts).
fn grounded() -> (SimState, Tune) {
    let t = tune();
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::Stand;
    f.ground_plat = 0;
    f.pos = Vector2::new(500.0, GROUND_Y);
    f.facing = 1.0;
    let g = &mut s.fighters[1];
    g.state = CharState::Stand;
    g.ground_plat = 0;
    g.pos = Vector2::new(200.0, GROUND_Y);
    (s, t)
}

/// Fighter 0 airborne mid-air, arm boost ready; fighter 1 parked far off (no footstool target).
fn airborne() -> (SimState, Tune) {
    let t = tune();
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::Air;
    f.pos = Vector2::new(500.0, 100.0);
    f.vel = Vector2::ZERO;
    f.ground_plat = -1;
    f.air_jumps = 1;
    let g = &mut s.fighters[1];
    g.state = CharState::Stand;
    g.ground_plat = 0;
    g.pos = Vector2::new(200.0, GROUND_Y);
    (s, t)
}

/// Count live projectiles of `kind` fired by fighter 0.
fn shots(s: &SimState, kind: ItemKind) -> usize {
    s.items
        .iter()
        .filter(|it| it.active() && it.kind == kind && it.owner == 0)
        .count()
}

fn has_fx(s: &SimState, kind: FxKind) -> bool {
    s.fx.iter().any(|x| x.kind == kind)
}

#[test]
fn touch_attach_replaces_the_body() {
    let (mut s, t) = grounded();
    // an AC core resting on fighter 0's feet: attach is TOUCH, not a button.
    s.items[0] = Item {
        kind: ItemKind::AcCore,
        pos: s.fighters[0].pos,
        owner: -1,
        ..Item::EMPTY
    };
    s = step(&s, &[&IDLE, &IDLE], &t);
    let f = &s.fighters[0];
    assert!(
        f.has_badge(Badge::AcCore),
        "the core body-replaced the fighter"
    );
    assert!(
        f.arm < crate::v1::ac::ARM_WEAPONS,
        "rolled a real arm weapon"
    );
    assert!(!s.items[0].active(), "the core is consumed on attach");
    assert!(
        has_fx(&s, FxKind::Transform),
        "the transform poof is recorded for the renderer"
    );
}

#[test]
fn quick_boost_is_a_jump_tap_on_cooldown() {
    let (mut s, t) = airborne();
    make_ac(&mut s.fighters[0], ArmWeapon::MachineGun as u8);
    // jump TAP (not held) + stick right: a quick boost burst, not a double jump.
    let tap = InputFrame {
        dir: 1.0,
        jump: true,
        ..IDLE
    };
    s = step(&s, &[&tap, &IDLE], &t);
    let f = s.fighters[0];
    assert!(
        (f.vel.x - t.ac_qb_speed).abs() < 10.0,
        "burst carried the quick-boost speed, vel.x={}",
        f.vel.x
    );
    assert!(f.qb_cd > 0, "the boost went on cooldown");
    assert_eq!(f.air_jumps, 1, "a quick boost spends no air jump");
    // a second tap while still on cooldown must NOT re-burst (opposite aim would flip vel.x).
    let tap_back = InputFrame {
        dir: -1.0,
        jump: true,
        ..IDLE
    };
    let s2 = step(&s, &[&tap_back, &IDLE], &t);
    assert!(s2.fighters[0].qb_cd > 0, "still cooling down");
    assert!(
        s2.fighters[0].vel.x > 0.0,
        "cooldown blocked the second burst, vel.x={}",
        s2.fighters[0].vel.x
    );
    assert_eq!(s2.fighters[0].air_jumps, 1);
}

#[test]
fn boost_climbs_against_extreme_gravity() {
    let (mut s, t) = airborne();
    make_ac(&mut s.fighters[0], ArmWeapon::MachineGun as u8);
    // hold jump, neutral stick: constant up thrust that out-fights the doubled gravity.
    let boost = InputFrame {
        jump_held: true,
        ..IDLE
    };
    for _ in 0..30 {
        s = step(&s, &[&boost, &IDLE], &t);
    }
    let f = s.fighters[0];
    assert!(f.vel.y < 0.0, "thrust wins: it climbs, vel.y={}", f.vel.y);
    assert!(
        f.vel.length() <= t.ac_boost_max + 2.0,
        "boost is speed-capped, |vel|={}",
        f.vel.length()
    );
}

#[test]
fn ac_falls_like_a_fridge() {
    let (mut s, t) = airborne();
    make_ac(&mut s.fighters[0], ArmWeapon::MachineGun as u8);
    // fighter 1 airborne too, but NOT AC'd — the control weight.
    s.fighters[1].pos = Vector2::new(700.0, 100.0);
    s.fighters[1].state = CharState::Air;
    s.fighters[1].ground_plat = -1;
    for _ in 0..15 {
        s = step(&s, &[&IDLE, &IDLE], &t);
    }
    assert!(
        s.fighters[0].vel.y > s.fighters[1].vel.y,
        "extreme gravity drops the mech faster: ac={} vs normal={}",
        s.fighters[0].vel.y,
        s.fighters[1].vel.y
    );
}

#[test]
fn cstick_fires_the_arm_not_a_smash() {
    let (mut s, t) = grounded();
    make_ac(&mut s.fighters[0], ArmWeapon::MachineGun as u8);
    let cadence = arm_spec(ArmWeapon::MachineGun).cadence;
    let flick = InputFrame { cx: 1.0, ..IDLE };
    // first deflected frame: the arm gun fires a bolt (never a smash).
    s = step(&s, &[&flick, &IDLE], &t);
    assert_eq!(
        shots(&s, ItemKind::LaserBolt),
        1,
        "the c-stick fired a bolt"
    );
    assert_eq!(s.fighters[0].arm_cd, cadence, "arm went on its cadence");
    assert_ne!(s.fighters[0].state, CharState::Fsmash);
    // hold the stick through the cadence: no re-fire until the cooldown reaches 0.
    for _ in 0..(cadence - 1) {
        s = step(&s, &[&flick, &IDLE], &t);
        assert_eq!(shots(&s, ItemKind::LaserBolt), 1, "no re-fire mid-cadence");
        assert_ne!(
            s.fighters[0].state,
            CharState::Fsmash,
            "c-stick never smashes an AC"
        );
    }
    // the frame the cooldown expires, continued deflection fires the second bolt.
    s = step(&s, &[&flick, &IDLE], &t);
    assert_eq!(
        shots(&s, ItemKind::LaserBolt),
        2,
        "a second bolt once off cooldown"
    );
    assert_ne!(s.fighters[0].state, CharState::Fsmash);
    // and the c-stick is never a smash macro, even released and idle.
    for _ in 0..20 {
        s = step(&s, &[&IDLE, &IDLE], &t);
        assert_ne!(s.fighters[0].state, CharState::Fsmash);
    }
}

#[test]
fn bazooka_round_explodes_on_a_body() {
    let (mut s, t) = grounded();
    make_ac(&mut s.fighters[0], ArmWeapon::Bazooka as u8);
    // a victim in line, ~200px to the right (the bazooka fires straight along the c-stick).
    s.fighters[1].pos = Vector2::new(700.0, GROUND_Y);
    let flick = InputFrame { cx: 1.0, ..IDLE };
    s = step(&s, &[&flick, &IDLE], &t);
    assert_eq!(
        shots(&s, ItemKind::Rocket),
        1,
        "the bazooka round is in flight"
    );
    // let the round travel into the body and detonate.
    for _ in 0..40 {
        if s.fighters[1].damage > 0.0 {
            break;
        }
        s = step(&s, &[&IDLE, &IDLE], &t);
    }
    assert!(
        s.fighters[1].damage > 0.0,
        "the rocket blast damaged the victim"
    );
    assert!(
        has_fx(&s, FxKind::Explosion),
        "the blast was recorded as an fx"
    );
}

// ── AC gas: the fuel meter that is, today, the only way out of the mech ────────────────────────

#[test]
fn attach_arms_600_frames_and_expiry_pops_you_out() {
    let (mut s, t) = airborne();
    // the real touch-attach path (not `make_ac`'s shortcut), so the fuel meter actually arms.
    s.items[0] = Item {
        kind: ItemKind::AcCore,
        pos: s.fighters[0].pos,
        owner: -1,
        ..Item::EMPTY
    };
    s = step(&s, &[&IDLE, &IDLE], &t);
    assert!(s.fighters[0].has_badge(Badge::AcCore), "attached");
    assert_eq!(
        s.fighters[0].ac_gas,
        crate::v1::ac::AC_GAS_FRAMES,
        "600 frames of fuel armed on attach"
    );
    for _ in 0..(crate::v1::ac::AC_GAS_FRAMES - 1) {
        s = step(&s, &[&IDLE, &IDLE], &t);
        assert!(
            s.fighters[0].has_badge(Badge::AcCore),
            "still armored mid-burn, ac_gas={}",
            s.fighters[0].ac_gas
        );
    }
    // the 600th stepped frame since attach: gas hits 0, badge clears.
    s = step(&s, &[&IDLE, &IDLE], &t);
    assert!(
        !s.fighters[0].has_badge(Badge::AcCore),
        "out of gas: popped back out of the mech"
    );
    assert_eq!(s.fighters[0].ac_gas, 0);

    // back to normal physics: no longer falls like a fridge (same comparator as
    // `ac_falls_like_a_fridge`, against a plain airborne control fighter).
    s.fighters[0].pos = Vector2::new(500.0, 100.0);
    s.fighters[0].vel = Vector2::ZERO;
    s.fighters[0].state = CharState::Air;
    s.fighters[0].ground_plat = -1;
    s.fighters[1].pos = Vector2::new(700.0, 100.0);
    s.fighters[1].vel = Vector2::ZERO;
    s.fighters[1].state = CharState::Air;
    s.fighters[1].ground_plat = -1;
    for _ in 0..15 {
        s = step(&s, &[&IDLE, &IDLE], &t);
    }
    assert!(
        (s.fighters[0].vel.y - s.fighters[1].vel.y).abs() < 1.0,
        "normal gravity now, no mech left to weigh it down: ac={} normal={}",
        s.fighters[0].vel.y,
        s.fighters[1].vel.y
    );
}

#[test]
fn respawn_clears_the_ac_badge_and_resets_gas() {
    let (mut s, t) = grounded();
    make_ac(&mut s.fighters[0], ArmWeapon::MachineGun as u8);
    s.fighters[0].ac_gas = 300; // mid-burn when the KO hits
    s.fighters[0].state = CharState::Air;
    s.fighters[0].ground_plat = -1;
    s.fighters[0].pos = Vector2::new(-4000.0, GROUND_Y); // past the blast zone
    s = step(&s, &[&IDLE, &IDLE], &t);
    let f = s.fighters[0];
    assert!(
        !f.has_badge(Badge::AcCore),
        "the mech does not survive a KO"
    );
    assert_eq!(f.ac_gas, 0, "fuel resets with the stock");
}

#[test]
fn all_badges_clear_on_respawn() {
    let (mut s, t) = grounded();
    s.fighters[0].badges = (Badge::Wings as u8) | (Badge::AcCore as u8);
    s.fighters[0].ac_gas = 450;
    s.fighters[0].state = CharState::Air;
    s.fighters[0].ground_plat = -1;
    s.fighters[0].pos = Vector2::new(-4000.0, GROUND_Y); // past the blast zone
    s = step(&s, &[&IDLE, &IDLE], &t);
    let f = s.fighters[0];
    assert_eq!(
        f.badges, 0,
        "every badge is stock-scoped -- a KO strips them all"
    );
    assert_eq!(f.ac_gas, 0);
}

#[test]
fn gas_does_not_tick_without_the_badge() {
    let (mut s, t) = grounded();
    s.fighters[0].ac_gas = 42; // a stray value; nothing should touch it absent the badge
    for _ in 0..10 {
        s = step(&s, &[&IDLE, &IDLE], &t);
    }
    assert!(!s.fighters[0].has_badge(Badge::AcCore));
    assert_eq!(s.fighters[0].ac_gas, 42, "gas is inert without the badge");
}
