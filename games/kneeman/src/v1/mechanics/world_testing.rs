//! Serializable multi-frame scenarios for runtime rule sets and relation semantics.

use super::{
    CaseInstance, CaseStep, Command, CommandTape, EntityId, Event, EventId, EventTape,
    InstanceView, MechanicsWorld, Phase, PlanError, Program, ProgramError, Relation, RelationError,
    RelationKindId, RuleSetId, ScriptState, SetError, StateError, WorldCommitError,
    commit_instance, commit_world, plan,
};
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum GraphExpectation {
    SetActive(RuleSetId),
    SetInactive(RuleSetId),
    Connected {
        kind: RelationKindId,
        from: EntityId,
        to: EntityId,
    },
    NotConnected {
        kind: RelationKindId,
        from: EntityId,
        to: EntityId,
    },
    Emitted {
        step: u16,
        kind: EventId,
        target: EntityId,
    },
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GraphCase {
    pub instance: CaseInstance,
    pub active_sets: Vec<RuleSetId>,
    pub relations: Vec<Relation>,
    pub steps: Vec<CaseStep>,
    pub expect: Vec<GraphExpectation>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GraphStepTrace {
    pub event: Event,
    pub active_sets_before: Vec<RuleSetId>,
    pub commands: Vec<Command>,
    pub emitted: Vec<Event>,
    pub active_sets_after: Vec<RuleSetId>,
    pub relations_after: Vec<Relation>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GraphTrace {
    pub initial: GraphCase,
    pub steps: Vec<GraphStepTrace>,
    pub final_state: ScriptState,
    pub final_active_sets: Vec<RuleSetId>,
    pub final_relations: Vec<Relation>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum GraphCaseError {
    InvalidProgram(Vec<ProgramError>),
    InitialRelation(RelationError),
    InitialSet(SetError),
    Plan {
        step: u16,
        phase: Phase,
        error: PlanError,
    },
    CommitInstance {
        step: u16,
        error: StateError,
    },
    CommitWorld {
        step: u16,
        error: WorldCommitError,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum GraphFailure {
    SetExpectedActive(RuleSetId),
    SetExpectedInactive(RuleSetId),
    MissingConnection {
        kind: RelationKindId,
        from: EntityId,
        to: EntityId,
    },
    UnexpectedConnection {
        kind: RelationKindId,
        from: EntityId,
        to: EntityId,
    },
    MissingEmission {
        step: u16,
        kind: EventId,
        target: EntityId,
    },
}

#[derive(Clone, PartialEq, Debug)]
pub struct GraphReport {
    pub trace: GraphTrace,
    pub failures: Vec<GraphFailure>,
}

impl GraphReport {
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Run the real planner, instance committer, and world committer with explicit fixed budgets.
pub fn run_graph_case<const COMMANDS: usize, const RELATIONS: usize, const EVENTS: usize>(
    program: &Program,
    case: &GraphCase,
) -> Result<GraphReport, GraphCaseError> {
    let errors = program.validate();
    if !errors.is_empty() {
        return Err(GraphCaseError::InvalidProgram(errors));
    }

    let mut world = MechanicsWorld::<RELATIONS>::new();
    for set in &case.active_sets {
        world
            .active_sets
            .add(*set)
            .map_err(GraphCaseError::InitialSet)?;
    }
    for relation in &case.relations {
        world
            .relations
            .add(*relation)
            .map_err(GraphCaseError::InitialRelation)?;
    }
    let mut state = case.instance.script;
    let mut traces = Vec::with_capacity(case.steps.len());

    for (step_index, step) in case.steps.iter().enumerate() {
        let step_index = step_index as u16;
        let state_before = state;
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
                    id: case.instance.id,
                    kind: case.instance.kind,
                    script: &state_before,
                    active_sets: world_before.active_sets,
                },
                step.event,
                phase,
                &mut commands,
            )
            .map_err(|error| GraphCaseError::Plan {
                step: step_index,
                phase,
                error,
            })?;
        }

        if let Err(error) = commit_instance(case.instance.id, &mut state, &commands) {
            return Err(GraphCaseError::CommitInstance {
                step: step_index,
                error,
            });
        }
        let mut emitted = EventTape::<EVENTS>::EMPTY;
        if let Err(error) = commit_world(&mut world, &commands, &mut emitted) {
            return Err(GraphCaseError::CommitWorld {
                step: step_index,
                error,
            });
        }
        world.frame = world.frame.wrapping_add(1);
        traces.push(GraphStepTrace {
            event: step.event,
            active_sets_before: world_before.active_sets.iter().collect(),
            commands: commands.as_slice().to_vec(),
            emitted: emitted.as_slice().to_vec(),
            active_sets_after: world.active_sets.iter().collect(),
            relations_after: world.relations.as_slice().to_vec(),
        });
    }

    let trace = GraphTrace {
        initial: case.clone(),
        steps: traces,
        final_state: state,
        final_active_sets: world.active_sets.iter().collect(),
        final_relations: world.relations.as_slice().to_vec(),
    };
    let failures = check_graph_expectations(&trace, &case.expect);
    Ok(GraphReport { trace, failures })
}

fn check_graph_expectations(
    trace: &GraphTrace,
    expectations: &[GraphExpectation],
) -> Vec<GraphFailure> {
    let mut failures = Vec::new();
    for expectation in expectations {
        match *expectation {
            GraphExpectation::SetActive(set) if !trace.final_active_sets.contains(&set) => {
                failures.push(GraphFailure::SetExpectedActive(set));
            }
            GraphExpectation::SetInactive(set) if trace.final_active_sets.contains(&set) => {
                failures.push(GraphFailure::SetExpectedInactive(set));
            }
            GraphExpectation::Connected { kind, from, to }
                if !has_relation(&trace.final_relations, kind, from, to) =>
            {
                failures.push(GraphFailure::MissingConnection { kind, from, to });
            }
            GraphExpectation::NotConnected { kind, from, to }
                if has_relation(&trace.final_relations, kind, from, to) =>
            {
                failures.push(GraphFailure::UnexpectedConnection { kind, from, to });
            }
            GraphExpectation::Emitted { step, kind, target }
                if !trace.steps.get(step as usize).is_some_and(|step| {
                    step.emitted
                        .iter()
                        .any(|event| event.kind == kind && event.target == target)
                }) =>
            {
                failures.push(GraphFailure::MissingEmission { step, kind, target });
            }
            _ => {}
        }
    }
    failures
}

fn has_relation(
    relations: &[Relation],
    kind: RelationKindId,
    from: EntityId,
    to: EntityId,
) -> bool {
    relations
        .iter()
        .any(|edge| edge.kind == kind && edge.from == from && edge.to == to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::mechanics::{
        KindId, Op, Operand, RelationEvents, Rule, Selector, Span, Target, Value,
    };

    const RULE_HOST: EntityId = EntityId(1);
    const FIRST: EntityId = EntityId(10);
    const SECOND: EntityId = EntityId(11);
    const SOCKET: EntityId = EntityId(20);
    const RULE_KIND: KindId = KindId(1);
    const HOLDS: RelationKindId = RelationKindId(1);
    const FILL_ONLY: RuleSetId = RuleSetId(1);
    const OVERWRITE: RuleSetId = RuleSetId(2);
    const SWITCH_SETS: EventId = EventId(1);
    const CONNECT: EventId = EventId(2);
    const CONNECTED: EventId = EventId(3);
    const BLOCKED: EventId = EventId(4);
    const DISPLACED: EventId = EventId(5);

    fn program() -> Program {
        let events = RelationEvents {
            connected: Some(CONNECTED),
            blocked: Some(BLOCKED),
            displaced: Some(DISPLACED),
        };
        let mut program = Program {
            rules: vec![
                Rule {
                    event: SWITCH_SETS,
                    phase: Phase::PrePhysics,
                    priority: 0,
                    source_order: 0,
                    selector: Selector {
                        kind: Some(RULE_KIND),
                        ..Selector::ANY
                    },
                    guards: Span { start: 0, len: 0 },
                    ops: Span { start: 0, len: 2 },
                },
                Rule {
                    event: CONNECT,
                    phase: Phase::Contact,
                    priority: 0,
                    source_order: 1,
                    selector: Selector {
                        kind: Some(RULE_KIND),
                        active_set: Some(FILL_ONLY),
                        ..Selector::ANY
                    },
                    guards: Span { start: 0, len: 0 },
                    ops: Span { start: 2, len: 1 },
                },
                Rule {
                    event: CONNECT,
                    phase: Phase::Contact,
                    priority: 0,
                    source_order: 2,
                    selector: Selector {
                        kind: Some(RULE_KIND),
                        active_set: Some(OVERWRITE),
                        ..Selector::ANY
                    },
                    guards: Span { start: 0, len: 0 },
                    ops: Span { start: 3, len: 1 },
                },
            ],
            guards: vec![],
            ops: vec![
                Op::RemoveActiveSet {
                    value: Operand::EventPayload(0),
                },
                Op::AddActiveSet {
                    value: Operand::EventPayload(1),
                },
                Op::SetRelationIfEmpty {
                    kind: HOLDS,
                    from: Target::EventSource,
                    to: Target::EventTarget,
                    events,
                },
                Op::SetRelation {
                    kind: HOLDS,
                    from: Target::EventSource,
                    to: Target::EventTarget,
                    events,
                },
            ],
        };
        program.normalize();
        program
    }

    fn case() -> GraphCase {
        GraphCase {
            instance: CaseInstance {
                id: RULE_HOST,
                kind: RULE_KIND,
                script: ScriptState::EMPTY,
            },
            active_sets: vec![FILL_ONLY],
            relations: vec![],
            steps: vec![
                CaseStep {
                    event: Event::new(CONNECT, FIRST, SOCKET),
                },
                CaseStep {
                    event: Event::new(CONNECT, SECOND, SOCKET),
                },
                CaseStep {
                    event: Event::new(SWITCH_SETS, RULE_HOST, RULE_HOST)
                        .with_payload(&[Value::RuleSet(FILL_ONLY), Value::RuleSet(OVERWRITE)])
                        .unwrap(),
                },
                CaseStep {
                    event: Event::new(CONNECT, SECOND, SOCKET),
                },
            ],
            expect: vec![
                GraphExpectation::SetActive(OVERWRITE),
                GraphExpectation::SetInactive(FILL_ONLY),
                GraphExpectation::Connected {
                    kind: HOLDS,
                    from: SECOND,
                    to: SOCKET,
                },
                GraphExpectation::NotConnected {
                    kind: HOLDS,
                    from: FIRST,
                    to: SOCKET,
                },
                GraphExpectation::Emitted {
                    step: 1,
                    kind: BLOCKED,
                    target: SECOND,
                },
                GraphExpectation::Emitted {
                    step: 3,
                    kind: DISPLACED,
                    target: FIRST,
                },
            ],
        }
    }

    #[test]
    fn runtime_set_switch_changes_graph_method_without_reloading_program() {
        let program = program();
        let case = case();
        let report = run_graph_case::<4, 4, 2>(&program, &case).unwrap();
        assert!(report.passed(), "{:#?}", report.failures);
        assert_eq!(report.trace.steps[0].emitted[0].kind, CONNECTED);
        assert_eq!(report.trace.steps[1].emitted[0].kind, BLOCKED);
        assert_eq!(report.trace.steps[2].active_sets_before, vec![FILL_ONLY]);
        assert_eq!(report.trace.steps[2].active_sets_after, vec![OVERWRITE]);
        assert_eq!(report.trace.steps[3].emitted.len(), 2);
        assert_eq!(report.trace.steps[3].emitted[0].kind, DISPLACED);
        assert_eq!(report.trace.steps[3].emitted[1].kind, CONNECTED);
    }

    #[test]
    fn graph_case_and_trace_replay_from_serialized_data() {
        let program = program();
        let case = case();
        let loaded_program: Program =
            bincode::deserialize(&bincode::serialize(&program).unwrap()).unwrap();
        let loaded_case: GraphCase =
            bincode::deserialize(&bincode::serialize(&case).unwrap()).unwrap();
        let first = run_graph_case::<4, 4, 2>(&loaded_program, &loaded_case).unwrap();
        let second = run_graph_case::<4, 4, 2>(&loaded_program, &loaded_case).unwrap();
        assert_eq!(first.trace, second.trace);
        assert_eq!(
            bincode::serialize(&first.trace).unwrap(),
            bincode::serialize(&second.trace).unwrap()
        );
    }
}
