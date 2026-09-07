//! Grounded-victim sentinel fixture and native trajectory check for browser snapshots.
use std::io::{self, Read, Write};

fn start() -> kneeman::SimState {
    let mut state = kneeman::fixtures::kick_travel(true);
    state.fighters[0].pos = kneeman::Vector2::new(900.0, 750.0);
    let victim = &mut state.fighters[1];
    victim.pos = kneeman::Vector2::new(924.0, 760.0); // main floor in this fixture
    victim.state = kneeman::CharState::Stand;
    victim.ground_plat = 0;
    state
}

fn main() {
    let mut state = start();
    match std::env::args().nth(1).as_deref() {
        None => io::stdout().lock().write_all(&bincode::serialize(&state).unwrap()).unwrap(),
        Some("--trace") => {
            let tune = kneeman::Tune::default();
            let mut legacy = tune.clone();
            for kit in std::sync::Arc::make_mut(&mut legacy.roster) {
                if let Some(attack) = kit.specials[3].air_hit.as_mut() {
                    for hit in &mut attack.boxes { hit.angle = 45.0; }
                }
            }
            let mut old = state;
            let mut rows = Vec::new();
            for _ in 0..60 {
                state = kneeman::step(&state, &[&kneeman::InputFrame::default(); 2], &tune);
                old = kneeman::step(&old, &[&kneeman::InputFrame::default(); 2], &legacy);
                let pos = state.fighters[1].pos;
                let previous = old.fighters[1].pos;
                rows.push((state.tick, [pos.x, pos.y], [previous.x, previous.y]));
            }
            println!("{}", serde_json::to_string(&rows).unwrap());
        }
        Some("--check") => {
            let mut bytes = Vec::new();
            io::stdin().read_to_end(&mut bytes).unwrap();
            let actual: kneeman::SimState = bincode::deserialize(&bytes).unwrap();
            assert!((1..=120).contains(&actual.tick), "expected an early browser snapshot");
            let tune = kneeman::Tune::default();
            while state.tick < actual.tick {
                state = kneeman::step(&state, &[&kneeman::InputFrame::default(); 2], &tune);
                if state.tick == 1 {
                    let vel = state.fighters[1].vel;
                    let angle = (-vel.y).atan2(vel.x).to_degrees();
                    assert!((angle - 44.0).abs() < 0.001, "first hit angle: {angle}");
                    assert_eq!(state.fighters[1].damage, 11.0);
                }
            }
            let expected = &state.fighters[1];
            let observed = &actual.fighters[1];
            assert_eq!(observed.damage, expected.damage);
            assert_eq!(observed.state, expected.state);
            assert!((observed.pos - expected.pos).length() < 0.01,
                "tick {}: browser {:?}, native {:?}", actual.tick, observed.pos, expected.pos);
            assert!((observed.vel - expected.vel).length() < 0.01,
                "tick {}: browser {:?}, native {:?}", actual.tick, observed.vel, expected.vel);
            println!("ground sentinel trajectory matches at tick {}: pos {:?}, vel {:?}",
                actual.tick, observed.pos, observed.vel);
        }
        Some(_) => panic!("expected --check, --trace or no argument"),
    }
}
