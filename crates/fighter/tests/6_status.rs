use game_fighter::{_1a_chart, _1b_ground, _1c_air, _5_status::runtime_inventory, Phase};

fn has_transition(
    entries: &[game_fighter::_5_status::Transition],
    from: Phase,
    event: &str,
    to: Option<Phase>,
) -> bool {
    entries
        .iter()
        .any(|entry| entry.from == from && entry.event == event && entry.to == to)
}

fn destinations(
    entries: &[game_fighter::_5_status::Transition],
    from: Phase,
    event: &str,
) -> Vec<Option<Phase>> {
    entries
        .iter()
        .filter(|entry| entry.from == from && entry.event == event)
        .map(|entry| entry.to)
        .collect()
}

#[test]
fn public_runtime_inventory_matches_phase_and_decision_outputs() {
    let inventory = runtime_inventory();
    assert_eq!(inventory.states, Phase::ALL.to_vec());
    assert_eq!(
        inventory.ground,
        game_fighter::_5_status::ground_transitions()
    );
    assert_eq!(inventory.air, game_fighter::_5_status::air_transitions());

    for phase in Phase::ALL {
        assert!(has_transition(
            &inventory.ground,
            phase,
            "JumpRequest",
            _1a_chart::decide(phase, _1a_chart::Event::JumpRequest),
        ));
        assert!(has_transition(
            &inventory.air,
            phase,
            "Land",
            _1a_chart::decide(phase, _1a_chart::Event::Land),
        ));
    }
}

#[test]
fn public_inventory_preserves_self_transitions_and_ordered_guards() {
    let inventory = runtime_inventory();
    assert!(has_transition(
        &inventory.ground,
        Phase::Dash,
        "Motion",
        Some(Phase::Dash),
    ));

    let all = _1b_ground::Facts {
        dash: true,
        walk: true,
        forward: true,
        reverse: true,
        down: true,
        finished: true,
        stopped: true,
    };
    assert_eq!(
        _1a_chart::decide(Phase::Dash, _1a_chart::Event::Motion(all)),
        Some(Phase::Dash)
    );
    assert_eq!(
        _1a_chart::decide(
            Phase::Dash,
            _1a_chart::Event::Motion(_1b_ground::Facts {
                reverse: false,
                ..all
            }),
        ),
        Some(Phase::Run),
    );
    assert_eq!(
        _1a_chart::decide(
            Phase::Dash,
            _1a_chart::Event::Motion(_1b_ground::Facts {
                reverse: false,
                forward: false,
                ..all
            }),
        ),
        Some(Phase::Brake),
    );

    let both_air_facts = _1c_air::AirFacts {
        descending: true,
        jump_pressed: true,
        jumps_left: 1,
    };
    assert_eq!(
        _1a_chart::decide(Phase::Jump, _1a_chart::Event::AirMotion(both_air_facts)),
        Some(Phase::AirJump),
    );
}

#[test]
fn public_inventory_has_exact_fact_totals_and_expected_transition_groups() {
    let inventory = runtime_inventory();
    for phase in Phase::ALL {
        for event in ["JumpRequest", "GroundIntent", "Motion"] {
            let facts: u32 = inventory
                .ground
                .iter()
                .filter(|entry| entry.from == phase && entry.event == event)
                .map(|entry| entry.witnesses)
                .sum();
            assert_eq!(facts, 128, "ground {phase:?} {event}");
        }
        for event in ["Motion", "Land"] {
            let facts: u32 = inventory
                .air
                .iter()
                .filter(|entry| entry.from == phase && entry.event == event)
                .map(|entry| entry.witnesses)
                .sum();
            assert_eq!(facts, 8, "air {phase:?} {event}");
        }
    }

    use Phase::*;
    assert_eq!(
        destinations(&inventory.ground, Dash, "Motion"),
        vec![Some(Brake), None, Some(Dash), Some(Run)],
    );
    assert_eq!(
        destinations(&inventory.ground, Run, "Motion"),
        vec![Some(Brake), None, Some(Turn), Some(CrouchEnter)],
    );
    assert_eq!(
        destinations(&inventory.air, Jump, "Motion"),
        vec![None, Some(Fall), Some(AirJump)],
    );
    assert_eq!(
        destinations(&inventory.air, Fall, "Motion"),
        vec![None, Some(AirJump)],
    );
}

#[test]
fn fact_partitions_make_guard_priority_reproducible() {
    let inventory = runtime_inventory();
    let dash = |to| {
        inventory
            .ground
            .iter()
            .find(|entry| entry.from == Phase::Dash && entry.event == "Motion" && entry.to == to)
            .unwrap()
            .fact_bits
            .clone()
    };
    assert_eq!(
        dash(Some(Phase::Dash)),
        (0..128u8).filter(|bits| bits & 8 != 0).collect::<Vec<_>>()
    );
    assert_eq!(
        dash(Some(Phase::Run)),
        (0..128u8)
            .filter(|bits| bits & 8 == 0 && bits & 4 != 0 && bits & 32 != 0)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        dash(Some(Phase::Brake)),
        (0..128u8)
            .filter(|bits| bits & 8 == 0 && bits & 4 == 0)
            .collect::<Vec<_>>()
    );
    let air_jump = inventory
        .air
        .iter()
        .find(|entry| {
            entry.from == Phase::Jump && entry.event == "Motion" && entry.to == Some(Phase::AirJump)
        })
        .unwrap();
    assert_eq!(air_jump.fact_bits, vec![6, 7]);
}
