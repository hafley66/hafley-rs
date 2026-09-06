use super::*;
use crate::v1::{CharData, CharState, SHIP_HOME, SHIP_R, Vector2};

const IDLE: InputFrame = InputFrame {
    dir: 0.0,
    aim_y: 0.0,
    cx: 0.0,
    cy: 0.0,
    jump: false,
    jump_held: false,
    shorthop: false,
    shield_held: false,
    shield_pressed: false,
    down: false,
    down_pressed: false,
    attack: false,
    attack_held: false,
    grab: false,
    special: false,
};

#[test]
fn ship_zero_gravity_cascade_replays_full_states_and_winner_traces() {
    let tune = Tune::from_char(&CharData::KNEEMAN);
    let mut prior = SimState::spawn();
    prior.fighters[0].pos = Vector2::new(SHIP_HOME.x, SHIP_HOME.y - SHIP_R - 40.0);
    prior.fighters[0].state = CharState::Air;
    let prior_bytes = bincode::serialize(&prior).unwrap();
    let rules = rules();
    let tape: Vec<_> = (0..90)
        .map(|frame| InputFrame {
            grab: frame == 60,
            cx: if frame > 60 { 1.0 } else { 0.0 },
            attack_held: (65..75).contains(&frame),
            ..IDLE
        })
        .collect();

    let record = |start: &SimState, inputs: &[InputFrame]| {
        let mut state = *start;
        inputs
            .iter()
            .map(|input| {
                let (next, debug) = step_receipt(&state, &[input, &IDLE], &tune, &rules);
                state = next;
                (bincode::serialize(&state).unwrap(), debug)
            })
            .collect::<Vec<_>>()
    };
    let first = record(&prior, &tape);
    assert_eq!(first, record(&prior, &tape));
    let restored: SimState = bincode::deserialize(&first[44].0).unwrap();
    assert_eq!(&first[45..], record(&restored, &tape[45..]));
    assert_eq!(bincode::serialize(&prior).unwrap(), prior_bytes);

    // The authored winner preserves the full executable V1 state sequence.
    let mut authority = prior;
    for (input, (bytes, _)) in tape.iter().zip(&first) {
        authority = crate::v1::step(&authority, &[input, &IDLE], &tune);
        assert_eq!(*bytes, bincode::serialize(&authority).unwrap());
    }

    // Whole ordered projection: lower-specificity declaration 2 precedes 1;
    // equal-specificity declaration 3 wins by Rust authoring order.
    let trace = &first[0].1;
    assert_eq!(
        (
            trace.entity,
            trace.property,
            trace
                .matching
                .iter()
                .map(|d| (d.source_order, d.precedence, d.value.0))
                .collect::<Vec<_>>(),
            trace.winner.map(|d| d.source_order)
        ),
        (
            SHIP_SLOT as u32,
            0,
            vec![
                (0, (0, 0), 1.0),
                (2, (0, 1), 1.0),
                (1, (0, 2), 0.25),
                (3, (0, 2), 0.0),
            ],
            Some(3)
        )
    );
    // Validate each retained file/line/column against the actual authoring source.
    let source = include_str!("0_ship.rs");
    let locations: Vec<_> = trace
        .matching
        .iter()
        .map(|d| {
            let line = source.lines().nth(d.location.line() as usize - 1).unwrap();
            (
                d.location.file().ends_with("content/rust/0_ship.rs"),
                d.location.column(),
                line.trim(),
            )
        })
        .collect();
    assert_eq!(
        locations,
        vec![
            (true, 11, "rules.set(GRAVITY_SCALE, GravityScale(1.0));"),
            (
                true,
                11,
                "rules.when(active_stroke, (0, 1), GRAVITY_SCALE, GravityScale(1.0));"
            ),
            (
                true,
                11,
                "rules.when(hull, (0, 2), GRAVITY_SCALE, GravityScale(0.25));"
            ),
            (
                true,
                11,
                "rules.when(hull, (0, 2), GRAVITY_SCALE, GravityScale(0.0));"
            ),
        ]
    );

    // A removed hull no longer selects either hull declaration or active-stroke row.
    let mut removed = prior;
    removed.paths[SHIP_SLOT].len = 0;
    let absent = rules.resolve(&removed, SHIP_SLOT as u32, GRAVITY_SCALE);
    assert_eq!(absent.matching, vec![trace.matching[0]]);

    // The resolved value is consumed by the real reducer: replacing the authored
    // scale changes the resulting state, while the input snapshot remains intact.
    let mut falling = Rules::default();
    falling.set(GRAVITY_SCALE, GravityScale(1.0));
    let (fall, _) = step_receipt(&prior, &[&IDLE, &IDLE], &tune, &falling);
    assert_ne!(bincode::serialize(&fall).unwrap(), first[0].0);
}
