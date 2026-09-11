//! Contact-path proofs: the Parry seam resolves through `game_combat`, the
//! sandbag only applies the resulting outcome, and snapshot/suffix replay keeps
//! every causally relevant combat value.
use super::*;
use std::sync::Arc;

/// Frozen strike values for the single synthetic attack frame.
const DAMAGE: f32 = 12.0;
const ANGLE: f32 = 45.0;
const BKB: u32 = 40;
const KBG: u32 = 70;

fn attack() -> Attack {
    Attack {
        id: 7,
        position: [0.0, 0.0, 0.0],
        radius: 1000.0,
        enabled: true,
        aerial: true,
        damage: DAMAGE,
        kbg: KBG,
        bkb: BKB,
        wdsk: 0,
        trajectory: ANGLE,
    }
}

fn frame(hit_boxes: Vec<Attack>) -> Frame {
    Frame {
        interruptible: false,
        landing_lag: false,
        x_pos: 0.0,
        y_pos: 0.0,
        hit_boxes,
    }
}

/// Wait, jump, and one-frame aerial attack. Only action 2 carries a hitbox, and
/// its radius guarantees the Parry overlap test fires on the attack tick.
fn actions() -> Arc<[Action]> {
    Arc::from(vec![
        Action {
            iasa: None,
            landing_lag: None,
            frames: vec![frame(vec![])],
        },
        Action {
            iasa: None,
            landing_lag: None,
            frames: vec![frame(vec![])],
        },
        Action {
            iasa: None,
            landing_lag: None,
            frames: vec![frame(vec![attack()])],
        },
    ])
}

/// The direct resolver result for the same strike/target pair the seam builds.
fn direct() -> game_combat::HitOutcome {
    game_combat::resolve_hit(
        game_combat::Strike {
            damage: DAMAGE,
            angle: ANGLE,
            base_knockback: BKB,
            knockback_growth: KBG,
            weight_dependent_set_knockback: 0,
        },
        game_combat::Target {
            percent: 0.0,
            weight: 100.0,
            grounded: false,
        },
        game_combat::DefenseInput { stick: [0.0, 0.0] },
        RESOLVE_POLICY,
    )
}

fn run_to_contact() -> World {
    let mut simulation = Simulation::new(actions(), true);
    for tick in 0..=78 {
        simulation.advance(fixture_input(tick));
    }
    simulation.state().clone()
}

/// The sandbag state after Parry reports contact equals calling `resolve_hit`
/// directly: no knockback, velocity, hitstun or damage is recomputed outside
/// the resolver.
#[test]
fn contact_result_matches_direct_resolve_hit() {
    let world = run_to_contact();
    assert_eq!(world.hit_count, 1);
    assert!(world.view.contact, "Parry reported no overlap");
    assert_eq!(world.view.hit, Some((7, DAMAGE)));

    let expected = direct();
    let bag = world.bag.as_ref().expect("launched sandbag");

    assert_eq!(world.movement.as_ref().unwrap().combat.percent, expected.percent_after);
    assert_eq!(bag.knockback, expected.knockback);
    assert_eq!(bag.stun, expected.hitstun);
    assert_eq!(
        bag.velocity,
        [0.0, expected.velocity[1], expected.velocity[0]]
    );
}

/// Save before contact, replay the suffix after load: damage, knockback,
/// hitstun, velocity, hit count and the contact outcome all survive.
#[test]
fn snapshot_suffix_replay_preserves_combat_outcome() {
    let tape: Vec<u8> = (0..=90).map(fixture_input).collect();
    let mut simulation = Simulation::new(actions(), true);
    for &bits in &tape[..70] {
        simulation.advance(bits);
    }
    let snapshot = simulation.save();

    let mut suffix = Vec::new();
    for &bits in &tape[70..] {
        suffix.push(simulation.advance(bits).clone());
    }
    simulation.load(&snapshot);
    for (offset, &bits) in tape[70..].iter().enumerate() {
        assert_eq!(simulation.advance(bits), &suffix[offset]);
    }

    let hit = suffix
        .iter()
        .find(|state| state.hit_count == 1)
        .expect("contact occurred in the suffix");
    let expected = direct();
    let bag = hit.bag.as_ref().expect("launched sandbag");
    assert_eq!(hit.movement.as_ref().unwrap().combat.percent, expected.percent_after);
    assert_eq!(bag.knockback, expected.knockback);
    assert_eq!(bag.stun, expected.hitstun);
    assert_eq!(
        bag.velocity,
        [0.0, expected.velocity[1], expected.velocity[0]]
    );
    assert!(hit.view.contact);
    assert_eq!(hit.view.hit, Some((7, DAMAGE)));
}
