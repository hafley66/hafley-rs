//! End-to-end runner for portable cases embedded in a [`MechanicsDocument`].

use super::{
    Command, CommandTape, DocumentCase, DocumentError, DocumentExpectation, EntityError, EntityId,
    EntityRecord, EntityTable, Event, EventTape, Field, InstanceView, MechanicsCommitError,
    MechanicsDocument, MechanicsWorld, Phase, PlanError, ProgramId, Relation, RelationError,
    RuleSetId, SetError, StateError, Value, WorldCommitError, commit_mechanics, plan,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct DocumentStepTrace {
    pub entity_before: EntityRecord,
    pub event: Event,
    pub active_sets_before: Vec<RuleSetId>,
    pub commands: Vec<Command>,
    pub emitted: Vec<Event>,
    pub entities_after: Vec<EntityRecord>,
    pub relations_after: Vec<Relation>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct DocumentTrace {
    pub initial: DocumentCase,
    pub steps: Vec<DocumentStepTrace>,
    pub final_entities: Vec<EntityRecord>,
    pub final_active_sets: Vec<RuleSetId>,
    pub final_relations: Vec<Relation>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum DocumentCaseError {
    InvalidDocument(Vec<DocumentError>),
    InitialEntity(EntityError),
    InitialRelation(RelationError),
    InitialSet(SetError),
    MissingEntity {
        step: u16,
        entity: EntityId,
    },
    MissingProgram {
        step: u16,
        program: ProgramId,
    },
    Plan {
        step: u16,
        phase: Phase,
        error: PlanError,
    },
    CommitEntity {
        step: u16,
        error: EntityError,
    },
    CommitWorld {
        step: u16,
        error: WorldCommitError,
    },
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum DocumentFailure {
    MissingEntity(EntityId),
    WrongPrefab {
        entity: EntityId,
        expected: super::PrefabId,
        actual: super::PrefabId,
    },
    WrongKind {
        entity: EntityId,
        expected: super::KindId,
        actual: super::KindId,
    },
    WrongProgram {
        entity: EntityId,
        expected: ProgramId,
        actual: ProgramId,
    },
    Field {
        entity: EntityId,
        field: Field,
        expected: Value,
        actual: Result<Value, StateError>,
    },
    MissingRelation(Relation),
}

#[derive(Clone, PartialEq, Debug)]
pub struct DocumentReport {
    pub trace: DocumentTrace,
    pub failures: Vec<DocumentFailure>,
}

impl DocumentReport {
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Execute one portable case against its loaded document using the real planner and committers.
pub fn run_document_case<
    const COMMANDS: usize,
    const ENTITIES: usize,
    const RELATIONS: usize,
    const EVENTS: usize,
>(
    document: &MechanicsDocument,
    case: &DocumentCase,
) -> Result<DocumentReport, DocumentCaseError> {
    let errors = document.validate();
    if !errors.is_empty() {
        return Err(DocumentCaseError::InvalidDocument(errors));
    }

    let mut entities = EntityTable::<ENTITIES>::EMPTY;
    for spawn in &case.entities {
        entities
            .spawn(document, spawn.id, spawn.prefab)
            .map_err(DocumentCaseError::InitialEntity)?;
    }
    let mut world = MechanicsWorld::<RELATIONS>::new();
    for set in &case.active_sets {
        world
            .active_sets
            .add(*set)
            .map_err(DocumentCaseError::InitialSet)?;
    }
    for relation in &case.relations {
        world
            .relations
            .add(*relation)
            .map_err(DocumentCaseError::InitialRelation)?;
    }

    let mut traces = Vec::with_capacity(case.steps.len());
    for (step_index, step) in case.steps.iter().enumerate() {
        let step_index = step_index as u16;
        let entity_before = *entities
            .get(step.entity)
            .ok_or(DocumentCaseError::MissingEntity {
                step: step_index,
                entity: step.entity,
            })?;
        let program =
            document
                .program(entity_before.program)
                .ok_or(DocumentCaseError::MissingProgram {
                    step: step_index,
                    program: entity_before.program,
                })?;
        let world_before = world;
        let mut commands = CommandTape::<COMMANDS>::EMPTY;
        for phase in [
            Phase::PrePhysics,
            Phase::Physics,
            Phase::Contact,
            Phase::PostPhysics,
            Phase::Effects,
        ] {
            plan(
                program,
                InstanceView {
                    id: entity_before.id,
                    kind: entity_before.kind,
                    script: &entity_before.script,
                    active_sets: world_before.active_sets,
                },
                step.event,
                phase,
                &mut commands,
            )
            .map_err(|error| DocumentCaseError::Plan {
                step: step_index,
                phase,
                error,
            })?;
        }

        // One event is one transaction across entity-local and world-level rollback facts.
        let mut emitted = EventTape::<EVENTS>::EMPTY;
        if let Err(error) =
            commit_mechanics(document, &mut entities, &mut world, &commands, &mut emitted)
        {
            return Err(match error {
                MechanicsCommitError::Entity(error) => DocumentCaseError::CommitEntity {
                    step: step_index,
                    error,
                },
                MechanicsCommitError::World(error) => DocumentCaseError::CommitWorld {
                    step: step_index,
                    error,
                },
            });
        }
        world.frame = world.frame.wrapping_add(1);
        traces.push(DocumentStepTrace {
            entity_before,
            event: step.event,
            active_sets_before: world_before.active_sets.iter().collect(),
            commands: commands.as_slice().to_vec(),
            emitted: emitted.as_slice().to_vec(),
            entities_after: entities.as_slice().to_vec(),
            relations_after: world.relations.as_slice().to_vec(),
        });
    }

    let trace = DocumentTrace {
        initial: case.clone(),
        steps: traces,
        final_entities: entities.as_slice().to_vec(),
        final_active_sets: world.active_sets.iter().collect(),
        final_relations: world.relations.as_slice().to_vec(),
    };
    let failures = check_expectations(&trace, &case.expect);
    Ok(DocumentReport { trace, failures })
}

fn check_expectations(
    trace: &DocumentTrace,
    expectations: &[DocumentExpectation],
) -> Vec<DocumentFailure> {
    let mut failures = Vec::new();
    let find = |id| trace.final_entities.iter().find(|entity| entity.id == id);
    for expectation in expectations {
        match *expectation {
            DocumentExpectation::EntityPrefab { entity, prefab } => match find(entity) {
                None => failures.push(DocumentFailure::MissingEntity(entity)),
                Some(actual) if actual.prefab != prefab => {
                    failures.push(DocumentFailure::WrongPrefab {
                        entity,
                        expected: prefab,
                        actual: actual.prefab,
                    })
                }
                Some(_) => {}
            },
            DocumentExpectation::EntityKind { entity, kind } => match find(entity) {
                None => failures.push(DocumentFailure::MissingEntity(entity)),
                Some(actual) if actual.kind != kind => failures.push(DocumentFailure::WrongKind {
                    entity,
                    expected: kind,
                    actual: actual.kind,
                }),
                Some(_) => {}
            },
            DocumentExpectation::EntityProgram { entity, program } => match find(entity) {
                None => failures.push(DocumentFailure::MissingEntity(entity)),
                Some(actual) if actual.program != program => {
                    failures.push(DocumentFailure::WrongProgram {
                        entity,
                        expected: program,
                        actual: actual.program,
                    })
                }
                Some(_) => {}
            },
            DocumentExpectation::FieldEquals {
                entity,
                field,
                value,
            } => match find(entity) {
                None => failures.push(DocumentFailure::MissingEntity(entity)),
                Some(actual) => {
                    let read = actual.script.read(field);
                    if read != Ok(value) {
                        failures.push(DocumentFailure::Field {
                            entity,
                            field,
                            expected: value,
                            actual: read,
                        });
                    }
                }
            },
            DocumentExpectation::Connected(relation)
                if !trace.final_relations.contains(&relation) =>
            {
                failures.push(DocumentFailure::MissingRelation(relation));
            }
            DocumentExpectation::Connected(_) => {}
        }
    }
    failures
}
