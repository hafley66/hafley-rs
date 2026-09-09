use smash::fighters::falcon;
use super::*;
use std::sync::Arc;

#[derive(serde::Deserialize)]
struct Recorded {
    world: falcon::World,
}
fn golden() -> Vec<[Recorded; 2]> {
    serde_json::from_slice(include_bytes!("17_launch_trace.json")).unwrap()
}

#[test]
fn incremental_peers_match_every_preexisting_full_state() {
    let actions = fixture::baseline::load().unwrap();
    let expected = golden();
    let mut runtime = Runtime::new(&actions, true, true).unwrap();
    assert_eq!(runtime.tick(), 0);
    for (tick, old_pair) in expected.iter().enumerate() {
        assert_eq!(runtime.tick(), tick);
        let pair = runtime
            .advance(falcon::fixture_input(tick as i32))
            .unwrap();
        assert_eq!(runtime.tick(), tick + 1);
        for (actual, old) in pair.iter().zip(old_pair) {
            assert_eq!(actual.world, old.world, "full-state mismatch at {tick}");
        }
    }
}

#[test]
fn core_save_load_replays_every_remaining_state_from_flight_and_ground() {
    let actions = fixture::baseline::load().unwrap();
    let baked: Arc<[falcon::Action]> = fixture::bake(&actions).into();
    let expected = golden();
    for checkpoint in [105, 128] {
        let mut sim = Simulation::new(baked.clone(), true);
        for tick in 0..checkpoint {
            sim.advance(falcon::fixture_input(tick));
        }
        let saved = sim.save();
        let before = sim.state().clone();
        for tick in checkpoint..180 {
            sim.advance(falcon::fixture_input(tick));
        }
        sim.load(&saved);
        assert_eq!(sim.state(), &before);
        for tick in checkpoint..180 {
            let state = sim.advance(falcon::fixture_input(tick));
            assert_eq!(
                state, &expected[tick as usize][0].world,
                "restored mismatch at {tick}"
            );
        }
    }
}

#[test]
fn input_is_applied_at_the_call_boundary() {
    let actions = fixture::baseline::load().unwrap();
    let mut sim = Simulation::new(fixture::bake(&actions).into(), true);
    for _ in 0..60 {
        sim.advance(0);
    }
    let saved = sim.save();
    let idle = sim.advance(0).view.clone();
    sim.load(&saved);
    let jump = sim.advance(1).view.clone();
    assert_eq!((idle.action, jump.action), (0, 1));
    assert_eq!(sim.state().frame, 61);
}
