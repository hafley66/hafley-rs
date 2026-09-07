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
            let floor_y = 760.0; // current main-floor fixture geometry
            fighter.pos.y = floor_y - 1.0;
            fighter.vel = kneeman::Vector2::new(0.0, 120.0);
            fighter.state = kneeman::CharState::SpecialD;
            fighter.frame = kneeman::Tune::default().for_char(2).specials[3].hit.startup + 1;
            fighter.hit_cd = [[100; kneeman::MAX_PLAYERS]; kneeman::MAX_HB];
            let grounded = mode == "--contact-ground";
            let victim = &mut state.fighters[1];
            victim.pos = kneeman::Vector2::new(960.0,
                if grounded { floor_y } else { floor_y - 10.0 });
            victim.state = if grounded { kneeman::CharState::Stand } else { kneeman::CharState::Air };
            victim.ground_plat = 0; // deliberately stale in the airborne control, like the native test
            victim.ground_ink = -1;
            victim.vel.y = if grounded { 0.0 } else { -60.0 };
            victim.invuln = 0;
        }
        Some(_) => panic!("expected --contact-ground or --contact-air, or no argument"),
    }
    let bytes = bincode::serialize(&state).expect("serialize kick landing fixture");
    io::stdout().lock().write_all(&bytes).expect("write kick landing fixture");
}
