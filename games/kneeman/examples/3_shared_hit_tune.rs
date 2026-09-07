//! Write a startup Tune whose three overlapping jab shapes share one hit identity.
use std::io::{self, Read, Write};

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--state") {
        let mut bytes = Vec::new();
        io::stdin().read_to_end(&mut bytes).expect("read browser snapshot");
        let state: kneeman::SimState = bincode::deserialize(&bytes).expect("decode browser snapshot");
        println!("[{},{}]", state.fighters[0].damage, state.fighters[1].damage);
        return;
    }
    let mut tune = kneeman::Tune::default();
    let hit = kneeman::Hitbox {
        id: 0, start: 0, len: 20, r: 1500.0, damage: 10.0,
        ..kneeman::Hitbox::NONE
    };
    tune.jab = kneeman::AttackData::new(0, 10, [hit, hit, hit, kneeman::Hitbox::NONE], 3);
    for character in std::sync::Arc::make_mut(&mut tune.roster).iter_mut() {
        character.jab = tune.jab;
    }
    let bytes = bincode::serialize(&tune).expect("serialize shared-hit Tune");
    io::stdout().lock().write_all(&bytes).expect("write shared-hit Tune");
}
