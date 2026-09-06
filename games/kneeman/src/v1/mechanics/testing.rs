//! Pure, serializable language scenarios.
//!
//! A case feeds authored events to one interpreted instance, records every planned command, and
//! checks declarative expectations. It intentionally uses the same `plan` and `commit_instance`
//! functions as the eventual world runtime: the harness is not a second interpreter.

use super::{
    ActiveSets, Command, CommandTape, EntityId, Event, EventId, Field, InstanceView, KindId, Phase,
    PlanError, Program, ProgramError, ScriptState, StateError, Value, commit_instance, plan,
};
use serde::{Deserialize, Serialize};

/// One instance under test. Multi-instance world scenarios belong in the world harness once the
/// interpreter is wired into `SimState`; this focused form pins language semantics today.
#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct CaseInstance {
    pub id: EntityId,
    pub kind: KindId,
    pub script: ScriptState,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct CaseStep {
    pub event: Event,
}

/// Expectations are data, not Rust closures, so an authoring shell can emit the same cases as the
/// program itself. `CommandAt` pins exact ordering; `Emitted` is less brittle when order is not the
/// subject of the test.
#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum Expectation {
    FieldEquals {
        field: Field,
        value: Value,
    },
    CommandAt {
        step: u16,
        index: u16,
        command: Command,
    },
    Emitted {
        step: u16,
        kind: EventId,
        target: EntityId,
    },
}

/// Portable test input. The normalized program is supplied separately so many cases can exercise
/// one program without duplicating its bytes.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct LanguageCase {
    pub instance: CaseInstance,
    pub steps: Vec<CaseStep>,
    pub expect: Vec<Expectation>,
}

impl Default for CaseInstance {
    fn default() -> Self {
        Self {
            id: EntityId(0),
            kind: KindId(0),
            script: ScriptState::EMPTY,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct StepTrace {
    pub event: Event,
    pub before: ScriptState,
    pub commands: Vec<Command>,
    pub after: ScriptState,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct CaseTrace {
    pub initial: CaseInstance,
    pub steps: Vec<StepTrace>,
    pub final_state: ScriptState,
}

#[derive(Clone, PartialEq, Debug)]
pub enum CaseError {
    InvalidProgram(Vec<ProgramError>),
    Plan {
        step: u16,
        phase: Phase,
        error: PlanError,
    },
    Commit {
        step: u16,
        error: StateError,
    },
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ExpectationFailure {
    Field {
        field: Field,
        expected: Value,
        actual: Result<Value, StateError>,
    },
    MissingStep {
        step: u16,
    },
    MissingCommand {
        step: u16,
        index: u16,
    },
    Command {
        step: u16,
        index: u16,
        expected: Command,
        actual: Command,
    },
    MissingEmission {
        step: u16,
        kind: EventId,
        target: EntityId,
    },
}

#[derive(Clone, PartialEq, Debug)]
pub struct CaseReport {
    pub trace: CaseTrace,
    pub failures: Vec<ExpectationFailure>,
}

impl CaseReport {
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Run a case with a compile-time command budget per event.
///
/// Every phase plans against the same pre-event snapshot, then self-directed state commands commit
/// once in canonical phase order. External commands and emitted events remain in the trace. They do
/// not automatically cascade: event-cascade ordering and its budget will be introduced explicitly
/// with the world interpreter.
pub fn run_case<const COMMANDS: usize>(
    program: &Program,
    case: &LanguageCase,
) -> Result<CaseReport, CaseError> {
    let errors = program.validate();
    if !errors.is_empty() {
        return Err(CaseError::InvalidProgram(errors));
    }

    let mut state = case.instance.script;
    let mut traces = Vec::with_capacity(case.steps.len());
    for (step_index, step) in case.steps.iter().enumerate() {
        let step_index = step_index as u16;
        let before = state;
        let mut tape = CommandTape::<COMMANDS>::EMPTY;
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
                    script: &before,
                    active_sets: ActiveSets::EMPTY,
                },
                step.event,
                phase,
                &mut tape,
            )
            .map_err(|error| CaseError::Plan {
                step: step_index,
                phase,
                error,
            })?;
        }
        commit_instance(case.instance.id, &mut state, &tape).map_err(|error| {
            CaseError::Commit {
                step: step_index,
                error,
            }
        })?;
        traces.push(StepTrace {
            event: step.event,
            before,
            commands: tape.as_slice().to_vec(),
            after: state,
        });
    }

    let trace = CaseTrace {
        initial: case.instance,
        steps: traces,
        final_state: state,
    };
    let failures = check_expectations(&trace, &case.expect);
    Ok(CaseReport { trace, failures })
}

fn check_expectations(trace: &CaseTrace, expectations: &[Expectation]) -> Vec<ExpectationFailure> {
    let mut failures = Vec::new();
    for expectation in expectations {
        match *expectation {
            Expectation::FieldEquals { field, value } => {
                let actual = trace.final_state.read(field);
                if actual != Ok(value) {
                    failures.push(ExpectationFailure::Field {
                        field,
                        expected: value,
                        actual,
                    });
                }
            }
            Expectation::CommandAt {
                step,
                index,
                command,
            } => match trace.steps.get(step as usize) {
                None => failures.push(ExpectationFailure::MissingStep { step }),
                Some(trace_step) => match trace_step.commands.get(index as usize) {
                    None => failures.push(ExpectationFailure::MissingCommand { step, index }),
                    Some(&actual) if actual != command => {
                        failures.push(ExpectationFailure::Command {
                            step,
                            index,
                            expected: command,
                            actual,
                        })
                    }
                    Some(_) => {}
                },
            },
            Expectation::Emitted { step, kind, target } => match trace.steps.get(step as usize) {
                None => failures.push(ExpectationFailure::MissingStep { step }),
                Some(trace_step)
                    if !trace_step.commands.iter().any(|command| {
                        matches!(command, Command::Emit(event) if event.kind == kind && event.target == target)
                    }) =>
                {
                    failures.push(ExpectationFailure::MissingEmission { step, kind, target })
                }
                Some(_) => {}
            },
        }
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::mechanics::{
        Compare, Guard, Op, Operand, Rule, Selector, Span, StateId, Target,
    };

    const TICK: EventId = EventId(1);
    const FINALIZED: EventId = EventId(2);
    const STROKE: KindId = KindId(8);
    const BUILDING: StateId = StateId(1);
    const DONE: StateId = StateId(2);

    fn countdown_program() -> Program {
        let mut program = Program {
            rules: vec![
                Rule {
                    event: TICK,
                    phase: Phase::PrePhysics,
                    priority: 0,
                    source_order: 0,
                    selector: Selector {
                        kind: Some(STROKE),
                        state: Some(BUILDING),
                        ..Selector::ANY
                    },
                    guards: Span { start: 0, len: 1 },
                    ops: Span { start: 0, len: 1 },
                },
                Rule {
                    event: TICK,
                    phase: Phase::PrePhysics,
                    priority: 10,
                    source_order: 1,
                    selector: Selector {
                        kind: Some(STROKE),
                        state: Some(BUILDING),
                        ..Selector::ANY
                    },
                    guards: Span { start: 1, len: 1 },
                    ops: Span { start: 1, len: 2 },
                },
            ],
            guards: vec![
                Guard {
                    left: Operand::SelfField(Field::Int(0)),
                    cmp: Compare::Gt,
                    right: Operand::Const(Value::Int(0)),
                },
                // Snapshot semantics: on the last positive tick, decrement and finalize are
                // planned together. No rule reads the decrement performed by its neighbor.
                Guard {
                    left: Operand::SelfField(Field::Int(0)),
                    cmp: Compare::Lte,
                    right: Operand::Const(Value::Int(1)),
                },
            ],
            ops: vec![
                Op::Add {
                    target: Target::SelfEntity,
                    field: Field::Int(0),
                    value: Operand::Const(Value::Int(-1)),
                },
                Op::Transition {
                    target: Target::SelfEntity,
                    state: DONE,
                },
                Op::Emit {
                    kind: FINALIZED,
                    target: Target::SelfEntity,
                },
            ],
        };
        program.normalize();
        program
    }

    fn countdown_case() -> LanguageCase {
        let id = EntityId(42);
        let mut script = ScriptState::EMPTY;
        script.state = BUILDING;
        script.ints[0] = 2;
        LanguageCase {
            instance: CaseInstance {
                id,
                kind: STROKE,
                script,
            },
            steps: vec![
                CaseStep {
                    event: Event::new(TICK, id, id),
                },
                CaseStep {
                    event: Event::new(TICK, id, id),
                },
            ],
            expect: vec![
                Expectation::FieldEquals {
                    field: Field::Int(0),
                    value: Value::Int(0),
                },
                Expectation::FieldEquals {
                    field: Field::State,
                    value: Value::State(DONE),
                },
                Expectation::Emitted {
                    step: 1,
                    kind: FINALIZED,
                    target: id,
                },
            ],
        }
    }

    #[test]
    fn authored_case_exercises_state_events_effects_and_snapshot_semantics() {
        let report = run_case::<8>(&countdown_program(), &countdown_case()).unwrap();
        assert!(report.passed(), "{:#?}", report.failures);
        assert_eq!(report.trace.steps[0].commands.len(), 1);
        assert_eq!(report.trace.steps[1].commands.len(), 3);
        assert_eq!(report.trace.steps[1].before.ints[0], 1);
        assert_eq!(report.trace.steps[1].after.state, DONE);
    }

    #[test]
    fn same_serializable_case_replays_to_identical_trace() {
        let program = countdown_program();
        let case = countdown_case();
        let bytes = bincode::serialize(&case).unwrap();
        let loaded: LanguageCase = bincode::deserialize(&bytes).unwrap();
        assert_eq!(loaded, case);

        let first = run_case::<8>(&program, &loaded).unwrap();
        let second = run_case::<8>(&program, &loaded).unwrap();
        assert_eq!(first.trace, second.trace);
        assert_eq!(
            bincode::serialize(&first.trace).unwrap(),
            bincode::serialize(&second.trace).unwrap()
        );
    }

    #[test]
    fn failures_are_data_with_useful_coordinates() {
        let mut case = countdown_case();
        case.expect.push(Expectation::CommandAt {
            step: 0,
            index: 0,
            command: Command::Despawn(case.instance.id),
        });
        let report = run_case::<8>(&countdown_program(), &case).unwrap();
        assert_eq!(report.failures.len(), 1);
        assert!(matches!(
            report.failures[0],
            ExpectationFailure::Command {
                step: 0,
                index: 0,
                ..
            }
        ));
    }

    #[test]
    fn the_case_budget_is_a_hard_failure() {
        let error = run_case::<2>(&countdown_program(), &countdown_case()).unwrap_err();
        assert_eq!(
            error,
            CaseError::Plan {
                step: 1,
                phase: Phase::PrePhysics,
                error: PlanError::Tape(super::super::TapeError::Full),
            }
        );
    }
}
