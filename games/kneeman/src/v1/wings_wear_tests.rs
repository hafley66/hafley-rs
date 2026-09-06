// Wings badge wear timer (queue-2026-07-03 item 6): the badge no longer persists until death --
// it now rides the generic per-badge wear table (`fighters::wear`), the DEFAULT de-spawn-after-
// pickup condition every future badge gets for free. Declared as a crate-root child so
// `super::*` is the crate root (same convention as `ac_tests`/`foundations_tests`).

use super::*;
use crate::v1::acts::attach_badge;
use crate::v1::fighters::wear::{DEFAULT_BADGE_WEAR_FRAMES, gas_for};

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

/// Attach the real path (`acts::attach_badge`), same applier the pickup and AcCore
/// touch both go through, so the wear timer arms exactly as it does in a real match.
#[test]
fn wings_expires_at_exactly_the_configured_frame() {
    let t = tune();
    let mut s = SimState::spawn();
    attach_badge(&mut s, 0, Badge::Wings);
    assert_eq!(
        gas_for(&s.fighters[0], Badge::Wings),
        DEFAULT_BADGE_WEAR_FRAMES,
        "600 frames of wear armed on attach"
    );
    for _ in 0..(DEFAULT_BADGE_WEAR_FRAMES - 1) {
        s = step(&s, &[&IDLE, &IDLE], &t);
        assert!(
            s.fighters[0].has_badge(Badge::Wings),
            "still winged mid-wear, gas={}",
            gas_for(&s.fighters[0], Badge::Wings)
        );
    }
    // the 600th stepped frame since attach: wear hits 0, badge clears.
    s = step(&s, &[&IDLE, &IDLE], &t);
    assert!(
        !s.fighters[0].has_badge(Badge::Wings),
        "worn out: badge cleared"
    );
    assert_eq!(gas_for(&s.fighters[0], Badge::Wings), 0);
}

/// After the wear timer runs out, the badge's `has_badge(Wings)` gate in za_warudo's air-jump
/// check goes false, so the ordinary single air jump governs again -- no more infinite hops.
#[test]
fn air_jumps_stop_being_infinite_after_expiry() {
    let t = tune();
    let mut s = SimState::spawn();
    attach_badge(&mut s, 0, Badge::Wings);
    for _ in 0..DEFAULT_BADGE_WEAR_FRAMES {
        s = step(&s, &[&IDLE, &IDLE], &t);
    }
    assert!(!s.fighters[0].has_badge(Badge::Wings), "expired");

    let f = &mut s.fighters[0];
    f.state = CharState::Air;
    f.ground_plat = -1;
    f.pos = Vector2::new(600.0, GROUND_Y - 400.0);
    f.vel = Vector2::ZERO;
    f.air_jumps = 1;
    let jump = InputFrame {
        jump: true,
        jump_held: true,
        ..IDLE
    };
    // the one ordinary air jump still works and spends itself.
    s = step(&s, &[&jump, &IDLE], &t);
    assert_eq!(
        s.fighters[0].air_jumps, 0,
        "the one real air jump was spent"
    );
    for _ in 0..12 {
        s = step(&s, &[&IDLE, &IDLE], &t); // fall a beat between presses
    }
    // No air jumps left and no Wings badge: a second press must be inert. Compare against a
    // control step with no jump input from the SAME state -- if the press mattered at all
    // (an infinite-jump pop), the two diverge; physics-only (gravity/drift) integrates
    // identically either way.
    let control = step(&s, &[&IDLE, &IDLE], &t);
    let pressed = step(&s, &[&jump, &IDLE], &t);
    assert!(
        (pressed.fighters[0].vel.y - control.fighters[0].vel.y).abs() < 0.01,
        "a jump press post-expiry changed vel.y ({} vs control {}): it granted another air jump",
        pressed.fighters[0].vel.y,
        control.fighters[0].vel.y
    );
    assert_eq!(
        pressed.fighters[0].air_jumps, 0,
        "still no air jumps banked"
    );
}

/// A KO still strips every badge immediately (`fighter::respawn` zeroing `badges`/`badge_gas`),
/// even mid-wear -- death is the OTHER way out, same as before this change.
#[test]
fn badge_still_clears_on_death_before_expiry() {
    let t = tune();
    let mut s = SimState::spawn();
    attach_badge(&mut s, 0, Badge::Wings);
    s.fighters[0].state = CharState::Air;
    s.fighters[0].ground_plat = -1;
    s.fighters[0].pos = Vector2::new(-4000.0, GROUND_Y); // past the blast zone
    let c = step(&s, &[&IDLE, &IDLE], &t);
    let f = c.fighters[0];
    assert!(
        !f.has_badge(Badge::Wings),
        "KO strips the badge before the wear timer ran out"
    );
    assert_eq!(
        gas_for(&f, Badge::Wings),
        0,
        "respawn zeroes the wear table too"
    );
}
