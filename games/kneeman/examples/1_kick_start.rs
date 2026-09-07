//! Write the airborne Falcon recovery-test start state as a bincode snapshot.
use std::io::{self, Write};

fn main() {
    let mut state = kneeman::SimState::spawn();
    let fighter = &mut state.fighters[0];
    fighter.char_id = 2;
    fighter.pos = kneeman::Vector2::new(1100.0, 0.0);
    fighter.state = kneeman::CharState::Air;
    fighter.ground_plat = -1;
    fighter.ground_ink = -1;
    let bytes = bincode::serialize(&state).expect("serialize kick fixture");
    io::stdout().lock().write_all(&bytes).expect("write kick fixture");
}
