use game_fighter::{Phase, air, ground, status::runtime_inventory};

fn has_transition(
    entries: &[game_fighter::status::Transition],
    from: Phase,
    event: &str,
    to: Option<Phase>,
) -> bool {
    entries
        .iter()
        .any(|entry| entry.from == from && entry.event == event && entry.to == to)
}

#[test]
fn public_runtime_inventory_matches_phase_and_decision_outputs() {
    let inventory = runtime_inventory();
    assert_eq!(inventory.states, Phase::ALL.to_vec());
    assert_eq!(inventory.ground, game_fighter::status::ground_transitions());
    assert_eq!(inventory.air, game_fighter::status::air_transitions());

    for phase in Phase::ALL {
        assert!(has_transition(
            &inventory.ground,
            phase,
            "JumpRequest",
            ground::decide(phase, ground::Event::JumpRequest),
        ));
        assert!(has_transition(
            &inventory.air,
            phase,
            "Land",
            air::decide(phase, air::AirEvent::Land),
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

    let all = ground::Facts {
        dash: true,
        walk: true,
        forward: true,
        reverse: true,
        down: true,
        finished: true,
        stopped: true,
    };
    assert_eq!(ground::decide(Phase::Dash, ground::Event::Motion(all)), Some(Phase::Dash));
    assert_eq!(
        ground::decide(
            Phase::Dash,
            ground::Event::Motion(ground::Facts { reverse: false, ..all }),
        ),
        Some(Phase::Run),
    );
    assert_eq!(
        ground::decide(
            Phase::Dash,
            ground::Event::Motion(ground::Facts {
                reverse: false,
                forward: false,
                ..all
            }),
        ),
        Some(Phase::Brake),
    );

    let both_air_facts = air::AirFacts {
        descending: true,
        jump_pressed: true,
        jumps_left: 1,
    };
    assert_eq!(
        air::decide(Phase::Jump, air::AirEvent::Motion(both_air_facts)),
        Some(Phase::AirJump),
    );
}
