use crate::v1::mechanics::{
    Command, EntityId, Field, KindId, MechanicsDocument, PrefabId, ProgramId, Value,
    run_document_case,
};

const FIXTURE: &str = include_str!("fixtures/dynamic-character.json");

#[test]
fn external_document_recomposes_character_without_losing_identity_or_relations() {
    let document: MechanicsDocument = serde_json::from_str(FIXTURE).unwrap();
    assert!(document.validate().is_empty());
    let case = &document.cases[0];
    let report = run_document_case::<4, 2, 2, 2>(&document, case).unwrap();
    assert!(report.passed(), "{:#?}", report.failures);

    let before_change = report.trace.steps[2].entity_before;
    assert_eq!(before_change.id, EntityId(100));
    assert_eq!(before_change.prefab, PrefabId(1));
    assert_eq!(before_change.script.read(Field::Int(0)), Ok(Value::Int(2)));
    assert_eq!(
        report.trace.steps[2].commands,
        vec![Command::Recompose {
            target: EntityId(100),
            prefab: PrefabId(2),
        }]
    );

    let after_change = report.trace.steps[2].entities_after[0];
    assert_eq!(after_change.id, EntityId(100));
    assert_eq!(after_change.prefab, PrefabId(2));
    assert_eq!(after_change.program, ProgramId(2));
    assert_eq!(after_change.kind, KindId(20));
    assert_eq!(after_change.script.read(Field::Int(0)), Ok(Value::Int(0)));

    let next_snapshot = report.trace.steps[3].entity_before;
    assert_eq!(next_snapshot, after_change);
    assert_eq!(
        report.trace.final_entities[0].script.read(Field::Int(0)),
        Ok(Value::Int(10))
    );
    assert_eq!(report.trace.final_relations, case.relations);
}

#[test]
fn loaded_document_and_case_replay_to_identical_serialized_trace() {
    let json_document: MechanicsDocument = serde_json::from_str(FIXTURE).unwrap();
    let loaded: MechanicsDocument =
        bincode::deserialize(&bincode::serialize(&json_document).unwrap()).unwrap();
    let first = run_document_case::<4, 2, 2, 2>(&loaded, &loaded.cases[0]).unwrap();
    let second = run_document_case::<4, 2, 2, 2>(&loaded, &loaded.cases[0]).unwrap();
    assert_eq!(first.trace, second.trace);
    assert_eq!(
        bincode::serialize(&first.trace).unwrap(),
        bincode::serialize(&second.trace).unwrap()
    );
}
