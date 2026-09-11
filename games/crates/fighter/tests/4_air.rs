use game_fighter::{
    _1c_air::{AirEvent, AirFacts, decide},
    Phase,
};

const AIRBORNE: [Phase; 3] = [Phase::Jump, Phase::AirJump, Phase::Fall];

fn expected_motion(phase: Phase, facts: AirFacts) -> Option<Phase> {
    use Phase::*;
    // Guard order mirrors _2_advance.rs: the pre-gravity jump check precedes the
    // post-gravity fall write, so a competing descending fact loses to jump.
    match phase {
        Jump | AirJump if facts.jump_pressed && facts.jumps_left > 0 => Some(AirJump),
        Jump | AirJump if facts.descending => Some(Fall),
        Fall if facts.jump_pressed && facts.jumps_left > 0 => Some(AirJump),
        _ => None,
    }
}

#[test]
fn motion_table_is_exhaustive_over_phases_and_fact_combinations() {
    for phase in Phase::ALL {
        for descending in [false, true] {
            for jump_pressed in [false, true] {
                for jumps_left in [0u8, 1, 2] {
                    let facts = AirFacts {
                        descending,
                        jump_pressed,
                        jumps_left,
                    };
                    assert_eq!(
                        decide(phase, AirEvent::Motion(facts)),
                        expected_motion(phase, facts),
                        "{phase:?} {facts:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn land_only_accepts_airborne_phases() {
    for phase in Phase::ALL {
        let expected = if AIRBORNE.contains(&phase) {
            Some(Phase::Landing)
        } else {
            None
        };
        assert_eq!(decide(phase, AirEvent::Land), expected, "{phase:?}");
    }
}

#[test]
fn competing_descending_and_jump_facts_resolve_in_callback_order() {
    let both = AirFacts {
        descending: true,
        jump_pressed: true,
        jumps_left: 1,
    };
    assert_eq!(
        decide(Phase::Jump, AirEvent::Motion(both)),
        Some(Phase::AirJump)
    );
    assert_eq!(
        decide(Phase::AirJump, AirEvent::Motion(both)),
        Some(Phase::AirJump)
    );
    assert_eq!(
        decide(Phase::Fall, AirEvent::Motion(both)),
        Some(Phase::AirJump)
    );

    let no_budget = AirFacts {
        jumps_left: 0,
        ..both
    };
    assert_eq!(
        decide(Phase::Jump, AirEvent::Motion(no_budget)),
        Some(Phase::Fall)
    );
    assert_eq!(
        decide(Phase::AirJump, AirEvent::Motion(no_budget)),
        Some(Phase::Fall)
    );
}

#[test]
fn serialized_air_phase_resumes_the_same_dispatch_tape() {
    let tape = [
        (
            Phase::Jump,
            AirFacts {
                descending: true,
                jump_pressed: true,
                jumps_left: 1,
            },
        ),
        (
            Phase::AirJump,
            AirFacts {
                descending: true,
                jump_pressed: false,
                jumps_left: 0,
            },
        ),
        (
            Phase::Fall,
            AirFacts {
                descending: true,
                jump_pressed: true,
                jumps_left: 2,
            },
        ),
        (
            Phase::Fall,
            AirFacts {
                descending: true,
                jump_pressed: false,
                jumps_left: 2,
            },
        ),
    ];
    for (phase, facts) in tape {
        let saved = serde_json::to_string(&phase).unwrap();
        let restored: Phase = serde_json::from_str(&saved).unwrap();
        assert_eq!(
            decide(phase, AirEvent::Motion(facts)),
            decide(restored, AirEvent::Motion(facts))
        );
        assert_eq!(
            decide(phase, AirEvent::Land),
            decide(restored, AirEvent::Land)
        );
    }
}
