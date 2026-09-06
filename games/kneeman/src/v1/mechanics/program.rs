use super::{
    ActiveSets, EntityId, EventId, Field, KindId, PrefabId, RelationKindId, RuleSetId, ScriptState,
    StateError, StateId, Value,
};
use crate::v1::Vector2;
use serde::{Deserialize, Serialize};

pub const EVENT_PAYLOAD: usize = 4;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[repr(u8)]
pub enum Phase {
    PrePhysics,
    Physics,
    Contact,
    PostPhysics,
    Effects,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Event {
    pub kind: EventId,
    pub source: EntityId,
    pub target: EntityId,
    pub payload: [Value; EVENT_PAYLOAD],
    pub payload_len: u8,
}

impl Event {
    pub const fn new(kind: EventId, source: EntityId, target: EntityId) -> Self {
        Self {
            kind,
            source,
            target,
            payload: [Value::None; EVENT_PAYLOAD],
            payload_len: 0,
        }
    }

    pub fn with_payload(mut self, values: &[Value]) -> Result<Self, PayloadError> {
        if values.len() > EVENT_PAYLOAD {
            return Err(PayloadError::TooLarge {
                len: values.len(),
                max: EVENT_PAYLOAD,
            });
        }
        self.payload[..values.len()].copy_from_slice(values);
        self.payload_len = values.len() as u8;
        Ok(self)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PayloadError {
    TooLarge { len: usize, max: usize },
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Target {
    SelfEntity,
    EventSource,
    EventTarget,
    Ref(u8),
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum Operand {
    Const(Value),
    SelfField(Field),
    EventPayload(u8),
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Compare {
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Guard {
    pub left: Operand,
    pub cmp: Compare,
    pub right: Operand,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Selector {
    pub kind: Option<KindId>,
    pub state: Option<StateId>,
    pub active_set: Option<RuleSetId>,
    pub flags_all: u32,
    pub flags_none: u32,
}

impl Selector {
    pub const ANY: Self = Self {
        kind: None,
        state: None,
        active_set: None,
        flags_all: 0,
        flags_none: 0,
    };

    fn matches(self, instance: InstanceView<'_>) -> bool {
        self.kind.is_none_or(|kind| kind == instance.kind)
            && self
                .state
                .is_none_or(|state| state == instance.script.state)
            && self
                .active_set
                .is_none_or(|set| instance.active_sets.contains(set))
            && instance.script.flags & self.flags_all == self.flags_all
            && instance.script.flags & self.flags_none == 0
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum Op {
    Set {
        target: Target,
        field: Field,
        value: Operand,
    },
    Add {
        target: Target,
        field: Field,
        value: Operand,
    },
    Mul {
        target: Target,
        field: Field,
        value: Operand,
    },
    Clamp {
        target: Target,
        field: Field,
        min: Operand,
        max: Operand,
    },
    Transition {
        target: Target,
        state: StateId,
    },
    Emit {
        kind: EventId,
        target: Target,
    },
    Spawn {
        prefab: PrefabId,
        parent: Target,
    },
    /// Replace an entity's authored composition while preserving its runtime identity.
    Recompose {
        target: Target,
        prefab: PrefabId,
    },
    Despawn {
        target: Target,
    },
    ApplyImpulse {
        target: Target,
        impulse: Operand,
        at: Operand,
    },
    Anchor {
        child: Target,
        host: Target,
        offset: Operand,
    },
    Release {
        target: Target,
    },
    AppendPathPoint {
        target: Target,
        point: Operand,
    },
    FinalizePath {
        target: Target,
    },
    SplitGeometry {
        target: Target,
    },
    AddActiveSet {
        value: Operand,
    },
    RemoveActiveSet {
        value: Operand,
    },
    AddRelation {
        kind: RelationKindId,
        from: Target,
        to: Target,
        events: RelationEvents,
    },
    SetRelationIfEmpty {
        kind: RelationKindId,
        from: Target,
        to: Target,
        events: RelationEvents,
    },
    SetRelation {
        kind: RelationKindId,
        from: Target,
        to: Target,
        events: RelationEvents,
    },
    RemoveRelation {
        kind: RelationKindId,
        from: Target,
        to: Target,
    },
}

/// Optional authored event names for graph outcomes. The native kernel decides the outcome; the
/// program decides which semantic events, if any, that outcome should produce.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct RelationEvents {
    pub connected: Option<EventId>,
    pub blocked: Option<EventId>,
    pub displaced: Option<EventId>,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Span {
    pub start: u16,
    pub len: u16,
}

impl Span {
    fn range(self, total: usize) -> Option<core::ops::Range<usize>> {
        let start = self.start as usize;
        let end = start.checked_add(self.len as usize)?;
        (end <= total).then_some(start..end)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Rule {
    pub event: EventId,
    pub phase: Phase,
    /// Higher priority plans later and therefore wins a same-field last-write conflict.
    pub priority: i16,
    pub source_order: u16,
    pub selector: Selector,
    pub guards: Span,
    pub ops: Span,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Program {
    pub rules: Vec<Rule>,
    pub guards: Vec<Guard>,
    pub ops: Vec<Op>,
}

impl Program {
    /// Canonical planning order. Stable sort preserves authored order when every key ties.
    pub fn normalize(&mut self) {
        self.rules
            .sort_by_key(|rule| (rule.phase, rule.priority, rule.source_order));
    }

    pub fn validate(&self) -> Vec<ProgramError> {
        let mut out = Vec::new();
        for (index, rule) in self.rules.iter().enumerate() {
            if rule.guards.range(self.guards.len()).is_none() {
                out.push(ProgramError::GuardSpan { rule: index });
            }
            if rule.ops.range(self.ops.len()).is_none() {
                out.push(ProgramError::OpSpan { rule: index });
            }
        }
        out
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ProgramError {
    GuardSpan { rule: usize },
    OpSpan { rule: usize },
}

#[derive(Copy, Clone)]
pub struct InstanceView<'a> {
    pub id: EntityId,
    pub kind: KindId,
    pub script: &'a ScriptState,
    pub active_sets: ActiveSets,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum WriteMode {
    Set,
    Add,
    Mul,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum Command {
    None,
    Write {
        mode: WriteMode,
        target: EntityId,
        field: Field,
        value: Value,
    },
    Clamp {
        target: EntityId,
        field: Field,
        min: Value,
        max: Value,
    },
    Transition {
        target: EntityId,
        state: StateId,
    },
    Emit(Event),
    Spawn {
        prefab: PrefabId,
        parent: EntityId,
    },
    Recompose {
        target: EntityId,
        prefab: PrefabId,
    },
    Despawn(EntityId),
    ApplyImpulse {
        target: EntityId,
        impulse: Vector2,
        at: Vector2,
    },
    Anchor {
        child: EntityId,
        host: EntityId,
        offset: Vector2,
    },
    Release(EntityId),
    AppendPathPoint {
        target: EntityId,
        point: Vector2,
    },
    FinalizePath(EntityId),
    SplitGeometry(EntityId),
    AddActiveSet(RuleSetId),
    RemoveActiveSet(RuleSetId),
    AddRelation {
        kind: RelationKindId,
        from: EntityId,
        to: EntityId,
        events: RelationEvents,
    },
    SetRelationIfEmpty {
        kind: RelationKindId,
        from: EntityId,
        to: EntityId,
        events: RelationEvents,
    },
    SetRelation {
        kind: RelationKindId,
        from: EntityId,
        to: EntityId,
        events: RelationEvents,
    },
    RemoveRelation {
        kind: RelationKindId,
        from: EntityId,
        to: EntityId,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TapeError {
    Full,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct CommandTape<const N: usize> {
    slots: [Command; N],
    len: u16,
}

impl<const N: usize> CommandTape<N> {
    pub const EMPTY: Self = Self {
        slots: [Command::None; N],
        len: 0,
    };

    pub fn push(&mut self, command: Command) -> Result<(), TapeError> {
        let index = self.len as usize;
        if index >= N {
            return Err(TapeError::Full);
        }
        self.slots[index] = command;
        self.len += 1;
        Ok(())
    }

    pub fn as_slice(&self) -> &[Command] {
        &self.slots[..self.len as usize]
    }
}

impl<const N: usize> Default for CommandTape<N> {
    fn default() -> Self {
        Self::EMPTY
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum PlanError {
    InvalidProgram,
    State(StateError),
    MissingPayload(u8),
    TypeMismatch,
    Tape(TapeError),
}

impl From<StateError> for PlanError {
    fn from(value: StateError) -> Self {
        Self::State(value)
    }
}

impl From<TapeError> for PlanError {
    fn from(value: TapeError) -> Self {
        Self::Tape(value)
    }
}

/// Plan matching rules without mutating the snapshot. The caller commits the resulting tape later.
pub fn plan<const N: usize>(
    program: &Program,
    instance: InstanceView<'_>,
    event: Event,
    phase: Phase,
    tape: &mut CommandTape<N>,
) -> Result<(), PlanError> {
    for rule in &program.rules {
        if rule.phase != phase || rule.event != event.kind || !rule.selector.matches(instance) {
            continue;
        }
        let guards = rule
            .guards
            .range(program.guards.len())
            .ok_or(PlanError::InvalidProgram)?;
        let mut matched = true;
        for guard in &program.guards[guards] {
            let left = resolve_operand(guard.left, instance, event)?;
            let right = resolve_operand(guard.right, instance, event)?;
            if !compare(left, guard.cmp, right) {
                matched = false;
                break;
            }
        }
        if !matched {
            continue;
        }
        let ops = rule
            .ops
            .range(program.ops.len())
            .ok_or(PlanError::InvalidProgram)?;
        for op in &program.ops[ops] {
            tape.push(plan_op(*op, instance, event)?)?;
        }
    }
    Ok(())
}

fn resolve_target(
    target: Target,
    instance: InstanceView<'_>,
    event: Event,
) -> Result<EntityId, PlanError> {
    match target {
        Target::SelfEntity => Ok(instance.id),
        Target::EventSource => Ok(event.source),
        Target::EventTarget => Ok(event.target),
        Target::Ref(index) => match instance.script.read(Field::Ref(index))? {
            Value::Entity(id) => Ok(id),
            _ => Err(PlanError::TypeMismatch),
        },
    }
}

fn resolve_operand(
    operand: Operand,
    instance: InstanceView<'_>,
    event: Event,
) -> Result<Value, PlanError> {
    match operand {
        Operand::Const(value) => Ok(value),
        Operand::SelfField(field) => Ok(instance.script.read(field)?),
        Operand::EventPayload(index)
            if index < event.payload_len && (index as usize) < EVENT_PAYLOAD =>
        {
            Ok(event.payload[index as usize])
        }
        Operand::EventPayload(index) => Err(PlanError::MissingPayload(index)),
    }
}

fn vector(value: Value) -> Result<Vector2, PlanError> {
    match value {
        Value::Vector(value) => Ok(value),
        _ => Err(PlanError::TypeMismatch),
    }
}

fn rule_set(value: Value) -> Result<RuleSetId, PlanError> {
    match value {
        Value::RuleSet(value) => Ok(value),
        _ => Err(PlanError::TypeMismatch),
    }
}

fn plan_op(op: Op, instance: InstanceView<'_>, event: Event) -> Result<Command, PlanError> {
    let write = |mode, target, field, value| {
        Ok(Command::Write {
            mode,
            target: resolve_target(target, instance, event)?,
            field,
            value: resolve_operand(value, instance, event)?,
        })
    };
    match op {
        Op::Set {
            target,
            field,
            value,
        } => write(WriteMode::Set, target, field, value),
        Op::Add {
            target,
            field,
            value,
        } => write(WriteMode::Add, target, field, value),
        Op::Mul {
            target,
            field,
            value,
        } => write(WriteMode::Mul, target, field, value),
        Op::Clamp {
            target,
            field,
            min,
            max,
        } => Ok(Command::Clamp {
            target: resolve_target(target, instance, event)?,
            field,
            min: resolve_operand(min, instance, event)?,
            max: resolve_operand(max, instance, event)?,
        }),
        Op::Transition { target, state } => Ok(Command::Transition {
            target: resolve_target(target, instance, event)?,
            state,
        }),
        Op::Emit { kind, target } => Ok(Command::Emit(Event::new(
            kind,
            instance.id,
            resolve_target(target, instance, event)?,
        ))),
        Op::Spawn { prefab, parent } => Ok(Command::Spawn {
            prefab,
            parent: resolve_target(parent, instance, event)?,
        }),
        Op::Recompose { target, prefab } => Ok(Command::Recompose {
            target: resolve_target(target, instance, event)?,
            prefab,
        }),
        Op::Despawn { target } => Ok(Command::Despawn(resolve_target(target, instance, event)?)),
        Op::ApplyImpulse {
            target,
            impulse,
            at,
        } => Ok(Command::ApplyImpulse {
            target: resolve_target(target, instance, event)?,
            impulse: vector(resolve_operand(impulse, instance, event)?)?,
            at: vector(resolve_operand(at, instance, event)?)?,
        }),
        Op::Anchor {
            child,
            host,
            offset,
        } => Ok(Command::Anchor {
            child: resolve_target(child, instance, event)?,
            host: resolve_target(host, instance, event)?,
            offset: vector(resolve_operand(offset, instance, event)?)?,
        }),
        Op::Release { target } => Ok(Command::Release(resolve_target(target, instance, event)?)),
        Op::AppendPathPoint { target, point } => Ok(Command::AppendPathPoint {
            target: resolve_target(target, instance, event)?,
            point: vector(resolve_operand(point, instance, event)?)?,
        }),
        Op::FinalizePath { target } => Ok(Command::FinalizePath(resolve_target(
            target, instance, event,
        )?)),
        Op::SplitGeometry { target } => Ok(Command::SplitGeometry(resolve_target(
            target, instance, event,
        )?)),
        Op::AddActiveSet { value } => Ok(Command::AddActiveSet(rule_set(resolve_operand(
            value, instance, event,
        )?)?)),
        Op::RemoveActiveSet { value } => Ok(Command::RemoveActiveSet(rule_set(resolve_operand(
            value, instance, event,
        )?)?)),
        Op::AddRelation {
            kind,
            from,
            to,
            events,
        } => Ok(Command::AddRelation {
            kind,
            from: resolve_target(from, instance, event)?,
            to: resolve_target(to, instance, event)?,
            events,
        }),
        Op::SetRelationIfEmpty {
            kind,
            from,
            to,
            events,
        } => Ok(Command::SetRelationIfEmpty {
            kind,
            from: resolve_target(from, instance, event)?,
            to: resolve_target(to, instance, event)?,
            events,
        }),
        Op::SetRelation {
            kind,
            from,
            to,
            events,
        } => Ok(Command::SetRelation {
            kind,
            from: resolve_target(from, instance, event)?,
            to: resolve_target(to, instance, event)?,
            events,
        }),
        Op::RemoveRelation { kind, from, to } => Ok(Command::RemoveRelation {
            kind,
            from: resolve_target(from, instance, event)?,
            to: resolve_target(to, instance, event)?,
        }),
    }
}

fn compare(left: Value, cmp: Compare, right: Value) -> bool {
    match cmp {
        Compare::Eq => left == right,
        Compare::Ne => left != right,
        Compare::Lt => ordered(left, right).is_some_and(|o| o.is_lt()),
        Compare::Lte => ordered(left, right).is_some_and(|o| o.is_le()),
        Compare::Gt => ordered(left, right).is_some_and(|o| o.is_gt()),
        Compare::Gte => ordered(left, right).is_some_and(|o| o.is_ge()),
    }
}

fn ordered(left: Value, right: Value) -> Option<core::cmp::Ordering> {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => Some(a.cmp(&b)),
        (Value::Scalar(a), Value::Scalar(b)) => a.partial_cmp(&b),
        _ => None,
    }
}

/// Apply commands aimed at one instance. Cross-entity commands and emitted events remain on the
/// tape for the world committer; this helper pins write/transition semantics in isolation.
pub fn commit_instance<const N: usize>(
    id: EntityId,
    state: &mut ScriptState,
    tape: &CommandTape<N>,
) -> Result<(), StateError> {
    for command in tape.as_slice() {
        match *command {
            Command::Write {
                mode,
                target,
                field,
                value,
            } if target == id => {
                let next = match mode {
                    WriteMode::Set => value,
                    WriteMode::Add => state
                        .read(field)?
                        .add(value)
                        .ok_or(StateError::InvalidArithmetic { field, rhs: value })?,
                    WriteMode::Mul => state
                        .read(field)?
                        .mul(value)
                        .ok_or(StateError::InvalidArithmetic { field, rhs: value })?,
                };
                state.write(field, next)?;
            }
            Command::Clamp {
                target,
                field,
                min,
                max,
            } if target == id => {
                let next = state
                    .read(field)?
                    .clamp(min, max)
                    .ok_or(StateError::InvalidClamp {
                        field,
                        lo: min,
                        hi: max,
                    })?;
                state.write(field, next)?;
            }
            Command::Transition {
                target,
                state: next,
            } if target == id => state.transition(next),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TICK: EventId = EventId(1);
    const KIND: KindId = KindId(4);

    fn fixture() -> Program {
        let mut program = Program {
            rules: vec![
                Rule {
                    event: TICK,
                    phase: Phase::PrePhysics,
                    priority: 20,
                    source_order: 1,
                    selector: Selector {
                        kind: Some(KIND),
                        ..Selector::ANY
                    },
                    guards: Span { start: 0, len: 1 },
                    ops: Span { start: 1, len: 2 },
                },
                Rule {
                    event: TICK,
                    phase: Phase::PrePhysics,
                    priority: 10,
                    source_order: 0,
                    selector: Selector {
                        kind: Some(KIND),
                        ..Selector::ANY
                    },
                    guards: Span { start: 0, len: 1 },
                    ops: Span { start: 0, len: 1 },
                },
            ],
            guards: vec![Guard {
                left: Operand::SelfField(Field::Scalar(0)),
                cmp: Compare::Gt,
                right: Operand::Const(Value::Scalar(0.0)),
            }],
            ops: vec![
                Op::Set {
                    target: Target::SelfEntity,
                    field: Field::Scalar(1),
                    value: Operand::Const(Value::Scalar(10.0)),
                },
                Op::Set {
                    target: Target::SelfEntity,
                    field: Field::Scalar(1),
                    value: Operand::Const(Value::Scalar(20.0)),
                },
                Op::Add {
                    target: Target::SelfEntity,
                    field: Field::Scalar(0),
                    value: Operand::Const(Value::Scalar(-1.0)),
                },
            ],
        };
        program.normalize();
        program
    }

    #[test]
    fn planning_reads_snapshot_and_higher_priority_commits_last() {
        let program = fixture();
        assert!(program.validate().is_empty());
        let mut state = ScriptState::EMPTY;
        state.scalars[0] = 2.0;
        let before = state;
        let id = EntityId(3);
        let mut tape = CommandTape::<8>::EMPTY;
        plan(
            &program,
            InstanceView {
                id,
                kind: KIND,
                script: &state,
                active_sets: ActiveSets::EMPTY,
            },
            Event::new(TICK, id, id),
            Phase::PrePhysics,
            &mut tape,
        )
        .unwrap();
        assert_eq!(state, before, "planning never mutates its snapshot");
        assert_eq!(tape.as_slice().len(), 3);
        commit_instance(id, &mut state, &tape).unwrap();
        assert_eq!(state.scalars[0], 1.0);
        assert_eq!(
            state.scalars[1], 20.0,
            "priority 20 writes after priority 10"
        );
    }

    #[test]
    fn command_tape_refuses_overflow() {
        let mut tape = CommandTape::<1>::EMPTY;
        tape.push(Command::Despawn(EntityId(1))).unwrap();
        assert_eq!(
            tape.push(Command::Despawn(EntityId(2))),
            Err(TapeError::Full)
        );
    }

    #[test]
    fn event_payload_refuses_truncation() {
        let values = [Value::Int(1); EVENT_PAYLOAD + 1];
        assert_eq!(
            Event::new(TICK, EntityId(1), EntityId(2)).with_payload(&values),
            Err(PayloadError::TooLarge {
                len: EVENT_PAYLOAD + 1,
                max: EVENT_PAYLOAD,
            })
        );
    }

    #[test]
    fn malformed_spans_fail_validation_and_planning() {
        let mut program = fixture();
        program.rules[0].ops = Span { start: 99, len: 1 };
        assert_eq!(program.validate(), vec![ProgramError::OpSpan { rule: 0 }]);
        let mut state = ScriptState::EMPTY;
        state.scalars[0] = 1.0; // satisfy the fixture guard so planning reaches the invalid op span
        let mut tape = CommandTape::<8>::EMPTY;
        let result = plan(
            &program,
            InstanceView {
                id: EntityId(1),
                kind: KIND,
                script: &state,
                active_sets: ActiveSets::EMPTY,
            },
            Event::new(TICK, EntityId(1), EntityId(1)),
            Phase::PrePhysics,
            &mut tape,
        );
        assert_eq!(result, Err(PlanError::InvalidProgram));
    }

    #[test]
    fn program_roundtrips_canonically() {
        let program = fixture();
        let bytes = bincode::serialize(&program).unwrap();
        let back: Program = bincode::deserialize(&bytes).unwrap();
        assert_eq!(back, program);
        assert_eq!(bincode::serialize(&back).unwrap(), bytes);
    }
}
