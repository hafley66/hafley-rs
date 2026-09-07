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
        Some(_) => panic!("expected --contact-ground or --contact-air, or no argument"),
    }
    let bytes = bincode::serialize(&state).expect("serialize kick landing fixture");
    io::stdout().lock().write_all(&bytes).expect("write kick landing fixture");
}
