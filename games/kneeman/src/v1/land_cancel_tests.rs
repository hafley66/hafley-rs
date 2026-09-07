// Landing-interrupt knob on attacks (queue-2026-07-03 item 3): a per-move `land_cancel` flag
// consulted by the ONE FSM site where an active attack's airborne state crosses onto ground
// (`land_transition`, wired into both `integrate_collide` landing sites). Declared as a
// crate-root child so `super::*` is the crate root.

use super::*;

fn tune() -> Tune {
    Tune::from_char(&CharData::KNEEMAN)
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

#[test]
fn special_recovery_maps_each_slot_to_its_own_recovery_clock() {
    let mut t = tune();
    for (slot, state) in [CharState::SpecialN, CharState::SpecialS, CharState::SpecialU, CharState::SpecialD].into_iter().enumerate() {
        t.specials[slot].hit.land_cancel = LandCancel::SpecialRecovery;
        t.specials[slot].hit.boxes[0].len += slot as i64;
        assert_eq!(land_transition(&t, state), (state, Some(t.specials[slot].hit.active_end())));
        t.specials[slot].hit.recovery = 0;
        assert_eq!(land_transition(&t, state), (CharState::Stand, Some(0)));
    }
    // Aerial states use their airborne integrator after landing, so this policy is specials-only.
    t.nair.land_cancel = LandCancel::SpecialRecovery;
    assert_eq!(land_transition(&t, CharState::Nair), (CharState::Landing, None));
    assert_eq!(land_transition(&t, CharState::Air), (CharState::Landing, None));
    for (tag, expected) in [(0u32, LandCancel::Continue), (1, LandCancel::ResetToLanding), (2, LandCancel::SpecialRecovery)] {
        let decoded: LandCancel = bincode::deserialize(&tag.to_le_bytes()).unwrap();
        assert_eq!(decoded, expected);
    }
}

/// Pin TODAY's behavior before touching anything: every attack-bearing state (the five aerials
/// plus the four specials) and every non-attack airborne state (Air/AirDodge/Helpless, which have
/// no `AttackData` at all) resets to `Landing` on touchdown. This is what
/// `za_warudo::integrate_collide` did unconditionally before this slice, so it is the
/// behavior-preserving default every move's `land_cancel` must carry.
#[test]
fn default_matches_todays_unconditional_landing_reset() {
    let t = tune();
    for st in [
        CharState::Nair,
        CharState::Fair,
        CharState::Bair,
        CharState::Uair,
        CharState::Dair,
        CharState::SpecialN,
        CharState::SpecialS,
        CharState::SpecialU,
        CharState::SpecialD,
        CharState::Air,
        CharState::AirDodge,
        CharState::Helpless,
    ] {
        assert_eq!(
            land_transition(&t, st),
            (CharState::Landing, None),
            "{st:?} should still reset to Landing by default"
        );
    }
}

/// The two variants differ ONLY in the flag: flip one move's `land_cancel` to `Continue` and the
/// same state that reset to `Landing` above now keeps running unchanged.
#[test]
fn continue_keeps_the_state_the_reset_variant_would_have_left() {
    let mut t = tune();
    assert_eq!(land_transition(&t, CharState::SpecialN), (CharState::Landing, None));
    t.specials[0].hit.land_cancel = LandCancel::Continue;
    assert_eq!(
        land_transition(&t, CharState::SpecialN),
        (CharState::SpecialN, None),
        "Continue keeps the act running in place instead of aborting to Landing"
    );
}

/// End-to-end: an aerial-launched neutral special lands mid-swing. With the default
/// (`ResetToLanding`) it aborts into `Landing`, same as any other airborne state landing today.
#[test]
fn reset_to_landing_variant_aborts_into_landing_through_the_real_fsm() {
    let t = tune();
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::SpecialN;
    f.ground_plat = -1;
    f.pos = Vector2::new(600.0, GROUND_Y - 4.0);
    f.vel = Vector2::new(0.0, 400.0); // already falling, about to cross the floor
    f.frame = 20; // past the hitbox window, still mid-recovery (not yet at `hit.total()`)
    let c = step(&s, &[&IDLE, &IDLE], &t);
    assert_eq!(
        c.fighters[0].state,
        CharState::Landing,
        "default land_cancel aborts the special into Landing on touchdown"
    );
}

/// Same setup, but the neutral special's `land_cancel` is flipped to `Continue` (Falcon Punch
/// style): touchdown no longer aborts the move -- the state stays `SpecialN`, now grounded
/// (`ground_plat` set), and keeps running out its own recovery.
#[test]
fn continue_variant_keeps_running_through_the_real_fsm() {
    let mut t = tune();
    t.specials[0].hit.land_cancel = LandCancel::Continue;
    let mut s = SimState::spawn();
    let f = &mut s.fighters[0];
    f.state = CharState::SpecialN;
    f.ground_plat = -1;
    f.pos = Vector2::new(600.0, GROUND_Y - 4.0);
    f.vel = Vector2::new(0.0, 400.0);
    f.frame = 20;
    let c = step(&s, &[&IDLE, &IDLE], &t);
    assert_eq!(
        c.fighters[0].state,
        CharState::SpecialN,
        "Continue keeps the special running instead of resetting to Landing"
    );
    assert!(
        c.fighters[0].ground_plat >= 0,
        "touchdown still pins the fighter to the floor it landed on"
    );
}
