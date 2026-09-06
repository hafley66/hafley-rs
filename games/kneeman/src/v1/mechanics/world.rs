use super::{
    ActiveSets, Command, CommandTape, EntityId, Event, EventId, Relation, RelationError,
    RelationEvents, RelationTable, RelationWriteOutcome, SetError, Value,
};

const EMPTY_EVENT: Event = Event::new(EventId(0), EntityId::NONE, EntityId::NONE);

/// Fixed output queue for events produced while committing world commands.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct EventTape<const N: usize> {
    slots: [Event; N],
    len: u16,
}

impl<const N: usize> EventTape<N> {
    pub const EMPTY: Self = Self {
        slots: [EMPTY_EVENT; N],
        len: 0,
    };

    pub fn push(&mut self, event: Event) -> Result<(), EventTapeError> {
        let index = self.len as usize;
        if index >= N {
            return Err(EventTapeError::Full);
        }
        self.slots[index] = event;
        self.len += 1;
        Ok(())
    }

    pub fn as_slice(&self) -> &[Event] {
        &self.slots[..self.len as usize]
    }
}

impl<const N: usize> Default for EventTape<N> {
    fn default() -> Self {
        Self::EMPTY
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum EventTapeError {
    Full,
}

/// The first world-level rollback facts needed by interpreted mechanics. Entities remain in their
/// existing arenas; this adds runtime rule-set selection and typed graph edges.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct MechanicsWorld<const RELATIONS: usize> {
    pub active_sets: ActiveSets,
    pub frame: u32,
    pub relations: RelationTable<RELATIONS>,
}

impl<const RELATIONS: usize> MechanicsWorld<RELATIONS> {
    pub const fn new() -> Self {
        Self {
            active_sets: ActiveSets::EMPTY,
            frame: 0,
            relations: RelationTable::EMPTY,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum WorldCommitError {
    Relations(RelationError),
    Sets(SetError),
    Events(EventTapeError),
}

impl From<RelationError> for WorldCommitError {
    fn from(value: RelationError) -> Self {
        Self::Relations(value)
    }
}

impl From<SetError> for WorldCommitError {
    fn from(value: SetError) -> Self {
        Self::Sets(value)
    }
}

impl From<EventTapeError> for WorldCommitError {
    fn from(value: EventTapeError) -> Self {
        Self::Events(value)
    }
}

/// Commit world commands transactionally. Failure restores both rollback facts and emitted events.
pub fn commit_world<const COMMANDS: usize, const RELATIONS: usize, const EVENTS: usize>(
    world: &mut MechanicsWorld<RELATIONS>,
    commands: &CommandTape<COMMANDS>,
    emitted: &mut EventTape<EVENTS>,
) -> Result<(), WorldCommitError> {
    let before = *world;
    let emitted_before = *emitted;
    let result = commit_world_inner(world, commands, emitted);
    if result.is_err() {
        *world = before;
        *emitted = emitted_before;
    }
    result
}

fn commit_world_inner<const COMMANDS: usize, const RELATIONS: usize, const EVENTS: usize>(
    world: &mut MechanicsWorld<RELATIONS>,
    commands: &CommandTape<COMMANDS>,
    emitted: &mut EventTape<EVENTS>,
) -> Result<(), WorldCommitError> {
    for command in commands.as_slice() {
        match *command {
            Command::Emit(event) => emitted.push(event)?,
            Command::AddActiveSet(set) => {
                world.active_sets.add(set)?;
            }
            Command::RemoveActiveSet(set) => {
                world.active_sets.remove(set)?;
            }
            Command::AddRelation {
                kind,
                from,
                to,
                events,
            } => {
                let outcome = world.relations.add(Relation {
                    kind,
                    from,
                    to,
                    since: world.frame,
                })?;
                emit_relation_outcome(emitted, events, outcome, from, to)?;
            }
            Command::SetRelationIfEmpty {
                kind,
                from,
                to,
                events,
            } => {
                let outcome = world.relations.set_if_empty(Relation {
                    kind,
                    from,
                    to,
                    since: world.frame,
                })?;
                emit_relation_outcome(emitted, events, outcome, from, to)?;
            }
            Command::SetRelation {
                kind,
                from,
                to,
                events,
            } => {
                let outcome = world.relations.set(Relation {
                    kind,
                    from,
                    to,
                    since: world.frame,
                })?;
                emit_relation_outcome(emitted, events, outcome, from, to)?;
            }
            Command::RemoveRelation { kind, from, to } => {
                world.relations.remove(kind, from, to);
            }
            _ => {}
        }
    }
    Ok(())
}

fn emit_relation_outcome<const N: usize>(
    tape: &mut EventTape<N>,
    names: RelationEvents,
    outcome: RelationWriteOutcome,
    from: EntityId,
    to: EntityId,
) -> Result<(), EventTapeError> {
    match outcome {
        RelationWriteOutcome::Connected => {
            emit_optional(tape, names.connected, from, from, &[Value::Entity(to)])
        }
        RelationWriteOutcome::AlreadyConnected => Ok(()),
        RelationWriteOutcome::Blocked { existing } => {
            emit_optional(tape, names.blocked, existing, from, &[Value::Entity(to)])
        }
        RelationWriteOutcome::Replaced { displaced } => {
            emit_optional(tape, names.displaced, from, displaced, &[Value::Entity(to)])?;
            emit_optional(tape, names.connected, from, from, &[Value::Entity(to)])
        }
    }
}

fn emit_optional<const N: usize>(
    tape: &mut EventTape<N>,
    kind: Option<EventId>,
    source: EntityId,
    target: EntityId,
    payload: &[Value],
) -> Result<(), EventTapeError> {
    let Some(kind) = kind else {
        return Ok(());
    };
    let event = Event::new(kind, source, target)
        .with_payload(payload)
        .expect("world outcome payload stays within the closed event budget");
    tape.push(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::mechanics::RelationKindId;

    #[test]
    fn world_commit_rolls_back_on_event_budget_failure() {
        let mut world = MechanicsWorld::<2>::new();
        let before = world;
        let mut commands = CommandTape::<1>::EMPTY;
        commands
            .push(Command::SetRelationIfEmpty {
                kind: RelationKindId(1),
                from: EntityId(1),
                to: EntityId(2),
                events: RelationEvents {
                    connected: Some(EventId(1)),
                    ..RelationEvents::default()
                },
            })
            .unwrap();
        let mut emitted = EventTape::<0>::EMPTY;
        assert_eq!(
            commit_world(&mut world, &commands, &mut emitted),
            Err(WorldCommitError::Events(EventTapeError::Full))
        );
        assert_eq!(world, before);
    }
}
