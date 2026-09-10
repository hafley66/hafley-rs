use game_fighter::{
    Phase,
    ground::{Event, Facts, decide},
};

#[test]
fn ground_jump_permissions_are_exclusive_and_air_landing_squat_reject() {
    use Phase::*;
    let phases = [
        Idle, Walk, Dash, Run, Brake, Turn, Squat, Crouch, Landing, Jump, Fall, AirJump,
    ];
    assert_eq!(
        phases.map(|phase| decide(phase, Event::JumpRequest)),
        [
            Some(Squat),
            Some(Squat),
            Some(Squat),
            Some(Squat),
            Some(Squat),
            Some(Squat),
            None,
            Some(Squat),
            None,
            None,
            None,
            None,
        ]
    );
}

#[test]
fn grounded_graph_has_ordered_guards_and_significant_dash_self_transition() {
    use Phase::*;
    let all = Facts {
        dash: true,
        walk: true,
        forward: true,
        reverse: true,
        down: true,
        finished: true,
        stopped: true,
    };
    let cases = [
        (Idle, Event::GroundIntent(all), Some(Crouch)),
        (
            Idle,
            Event::GroundIntent(Facts { down: false, ..all }),
            Some(Dash),
        ),
        (
            Idle,
            Event::GroundIntent(Facts {
                down: false,
                dash: false,
                ..all
            }),
            Some(Walk),
        ),
        (Dash, Event::Motion(all), Some(Dash)),
        (
            Dash,
            Event::Motion(Facts {
                reverse: false,
                ..all
            }),
            Some(Run),
        ),
        (
            Dash,
            Event::Motion(Facts {
                reverse: false,
                forward: false,
                ..all
            }),
            Some(Brake),
        ),
        (Run, Event::Motion(all), Some(Turn)),
        (
            Run,
            Event::Motion(Facts {
                reverse: false,
                walk: false,
                ..all
            }),
            Some(Brake),
        ),
        (
            Run,
            Event::Motion(Facts {
                reverse: false,
                ..all
            }),
            Some(Crouch),
        ),
        (Turn, Event::Motion(all), Some(Dash)),
        (
            Turn,
            Event::Motion(Facts {
                reverse: false,
                ..all
            }),
            Some(Idle),
        ),
        (Brake, Event::Motion(all), Some(Idle)),
        (Squat, Event::Motion(all), Some(Jump)),
        (Landing, Event::Motion(all), Some(Idle)),
        (Crouch, Event::Motion(all), None),
        (
            Crouch,
            Event::Motion(Facts { down: false, ..all }),
            Some(Idle),
        ),
        (Fall, Event::Motion(all), None),
    ];
    for (phase, event, expected) in cases {
        assert_eq!(decide(phase, event), expected, "{phase:?}");
    }
    assert_eq!(
        decide(
            Dash,
            Event::Motion(Facts {
                forward: true,
                ..Facts::default()
            })
        ),
        None
    );
}

#[test]
fn serialized_phase_resumes_the_same_statig_dispatch_tape() {
    let mut phase = Phase::Idle;
    let inputs = [1, 1, -1, 0, 0, 1, -1];
    for axis in inputs {
        let saved = serde_json::to_string(&phase).unwrap();
        let restored: Phase = serde_json::from_str(&saved).unwrap();
        let facts = Facts {
            dash: axis != 0,
            walk: axis != 0,
            forward: axis > 0,
            reverse: axis < 0,
            stopped: axis == 0,
            finished: true,
            ..Facts::default()
        };
        let original = decide(phase, Event::Motion(facts));
        assert_eq!(original, decide(restored, Event::Motion(facts)));
        let original = original.or_else(|| decide(phase, Event::GroundIntent(facts)));
        if let Some(next) = original {
            phase = next;
        }
    }
}
