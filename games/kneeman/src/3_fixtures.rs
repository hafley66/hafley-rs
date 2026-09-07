//! Fixed simulation starts shared by the replay debugger and browser fixture writers.
use crate::v1::*;

pub fn kick_contact(grounded: bool) -> SimState {
    let mut state = SimState::spawn();
    let fighter = &mut state.fighters[0];
    fighter.char_id = 2;
    fighter.pos = Vector2::new(900.0, GROUND_Y - 1.0);
    fighter.vel = Vector2::new(0.0, 120.0);
    fighter.state = CharState::SpecialD;
    fighter.special_started_air = true;
    fighter.frame = Tune::default().for_char(2).specials[3].hit.startup + 1;
    fighter.ground_plat = -1;
    fighter.ground_ink = -1;
    fighter.air_jumps = 0;
    fighter.hit_cd = [[100; MAX_PLAYERS]; MAX_HB];
    let victim = &mut state.fighters[1];
    victim.pos = Vector2::new(960.0, if grounded { GROUND_Y } else { GROUND_Y - 10.0 });
    victim.state = if grounded { CharState::Stand } else { CharState::Air };
    victim.ground_plat = 0; // intentionally stale in the airborne target-filter control
    victim.ground_ink = -1;
    victim.vel.y = if grounded { 0.0 } else { -60.0 };
    victim.invuln = 0;
    state
}
