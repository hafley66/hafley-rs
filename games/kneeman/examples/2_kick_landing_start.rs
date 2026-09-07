//! Write an airborne Falcon start above the main floor for browser landing acceptance.
//! --contact-ground / --contact-air select the native contact test's overlapping victim controls.
use std::io::{self, Write};

fn main() {
    let mut state = kneeman::SimState::spawn();
    let fighter = &mut state.fighters[0];
    fighter.char_id = 2;
    fighter.pos = kneeman::Vector2::new(900.0, 580.0);
    fighter.state = kneeman::CharState::Air;
    fighter.ground_plat = -1;
    fighter.ground_ink = -1;
    fighter.air_jumps = 0;
    match std::env::args().nth(1).as_deref() {
        None => {}
        Some(mode @ ("--contact-ground" | "--contact-air")) => {
            state = kneeman::fixtures::kick_contact(mode == "--contact-ground");
        }
        Some(mode @ ("--travel-ground" | "--travel-air")) => {
            state = kneeman::fixtures::kick_contact(false);
            let fighter = &mut state.fighters[0];
            fighter.special_started_air = mode == "--travel-air";
            fighter.pos = kneeman::Vector2::new(600.0, 400.0);
            fighter.vel = kneeman::Vector2::ZERO;
            fighter.frame = 14; // next simulation step enters the late travel phase
            fighter.hit_cd = [[0; kneeman::MAX_PLAYERS]; kneeman::MAX_HB];
            state.fighters[1].pos = kneeman::Vector2::new(624.0, 410.0);
            state.fighters[1].vel = kneeman::Vector2::ZERO;
            state.fighters[1].ground_plat = -1;
        }
        Some(_) => panic!("expected --contact-ground, --contact-air, --travel-ground, --travel-air, or no argument"),
    }
    let bytes = bincode::serialize(&state).expect("serialize kick landing fixture");
    io::stdout().lock().write_all(&bytes).expect("write kick landing fixture");
}
