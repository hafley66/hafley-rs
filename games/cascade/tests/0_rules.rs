use cascade::{DebugFrame, DeclarationTrace, GravityScale, Rules, GRAVITY_SCALE};

#[test]
fn immutable_selection_layer_specificity_and_source_order() {
    let mut rules = Rules::<bool>::default();
    assert_eq!(
        rules.resolve(&true, 7, GRAVITY_SCALE),
        DebugFrame {
            entity: 7,
            property: 0,
            matching: vec![],
            winner: None,
        }
    );
    rules.set(GRAVITY_SCALE, GravityScale(1.0));
    let authored_line = line!() + 1;
    rules.when(
        |active, entity| *active && entity == 7,
        (1, 0),
        GRAVITY_SCALE,
        GravityScale(0.0),
    );
    rules.when(|_, _| true, (0, 100), GRAVITY_SCALE, GravityScale(2.0));
    rules.when(
        |active, _| *active,
        (0, 100),
        GRAVITY_SCALE,
        GravityScale(3.0),
    );
    let frame = rules.resolve(&true, 7, GRAVITY_SCALE);
    let project = |matching: &[DeclarationTrace]| {
        matching
            .iter()
            .map(|declaration| {
                (
                    declaration.source_order,
                    declaration.precedence,
                    declaration.value.0,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        (
            project(&frame.matching),
            frame.winner.map(|d| d.source_order)
        ),
        (
            vec![
                (0, (0, 0), 1.0),
                (2, (0, 100), 2.0),
                (3, (0, 100), 3.0),
                (1, (1, 0), 0.0)
            ],
            Some(1)
        )
    );
    let location = frame.winner.unwrap().location;
    assert_eq!((location.file(), location.line()), (file!(), authored_line));
    assert_eq!(rules.resolve(&true, 7, GRAVITY_SCALE), frame);
    let inactive = rules.resolve(&false, 7, GRAVITY_SCALE);
    let other_entity = rules.resolve(&true, 8, GRAVITY_SCALE);
    assert_eq!(
        (project(&inactive.matching), project(&other_entity.matching)),
        (
            vec![(0, (0, 0), 1.0), (2, (0, 100), 2.0)],
            vec![(0, (0, 0), 1.0), (2, (0, 100), 2.0), (3, (0, 100), 3.0)]
        )
    );
}
