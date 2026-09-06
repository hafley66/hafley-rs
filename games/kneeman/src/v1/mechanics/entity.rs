//! Bounded rollback storage for interpreted entities.

use super::{
    Command, CommandTape, EntityId, EventTape, MechanicsDocument, MechanicsWorld, PrefabDefinition,
    PrefabId, ProgramId, ScriptState, StateError, WorldCommitError, commit_instance, commit_world,
};
use crate::v1::mechanics::KindId;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct EntityRecord {
    pub id: EntityId,
    pub prefab: PrefabId,
    pub program: ProgramId,
    pub kind: KindId,
    pub script: ScriptState,
}

impl EntityRecord {
    const EMPTY: Self = Self {
        id: EntityId::NONE,
        prefab: PrefabId(0),
        program: ProgramId(0),
        kind: KindId(0),
        script: ScriptState::EMPTY,
    };

    fn from_prefab(id: EntityId, prefab: PrefabDefinition) -> Self {
        Self {
            id,
            prefab: prefab.id,
            program: prefab.program,
            kind: prefab.kind,
            script: prefab.initial,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct EntityTable<const N: usize> {
    slots: [EntityRecord; N],
    len: u16,
}

impl<const N: usize> EntityTable<N> {
    pub const EMPTY: Self = Self {
        slots: [EntityRecord::EMPTY; N],
        len: 0,
    };

    pub fn as_slice(&self) -> &[EntityRecord] {
        &self.slots[..self.len as usize]
    }

    pub fn get(&self, id: EntityId) -> Option<&EntityRecord> {
        self.as_slice().iter().find(|entity| entity.id == id)
    }

    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut EntityRecord> {
        let index = self.as_slice().iter().position(|entity| entity.id == id)?;
        Some(&mut self.slots[index])
    }

    pub fn spawn(
        &mut self,
        document: &MechanicsDocument,
        id: EntityId,
        prefab: PrefabId,
    ) -> Result<(), EntityError> {
        if id.is_none() {
            return Err(EntityError::ReservedId);
        }
        if self.get(id).is_some() {
            return Err(EntityError::Duplicate(id));
        }
        let definition = *document
            .prefab(prefab)
            .ok_or(EntityError::MissingPrefab(prefab))?;
        let index = self.len as usize;
        if index >= N || self.len == u16::MAX {
            return Err(EntityError::Full);
        }
        self.slots[index] = EntityRecord::from_prefab(id, definition);
        self.len += 1;
        Ok(())
    }

    /// Swap authored composition, resetting prefab-local script residue. Entity identity and facts
    /// stored outside the record (relations, ownership edges, shell handles) are deliberately kept.
    pub fn recompose(
        &mut self,
        document: &MechanicsDocument,
        id: EntityId,
        prefab: PrefabId,
    ) -> Result<(), EntityError> {
        let definition = *document
            .prefab(prefab)
            .ok_or(EntityError::MissingPrefab(prefab))?;
        let entity = self.get_mut(id).ok_or(EntityError::Missing(id))?;
        *entity = EntityRecord::from_prefab(id, definition);
        Ok(())
    }

    pub fn despawn(&mut self, id: EntityId) -> Result<(), EntityError> {
        let index = self
            .as_slice()
            .iter()
            .position(|entity| entity.id == id)
            .ok_or(EntityError::Missing(id))?;
        let last = self.len as usize - 1;
        for slot in index..last {
            self.slots[slot] = self.slots[slot + 1];
        }
        self.slots[last] = EntityRecord::EMPTY;
        self.len -= 1;
        Ok(())
    }
}

impl<const N: usize> Default for EntityTable<N> {
    fn default() -> Self {
        Self::EMPTY
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum EntityError {
    Full,
    ReservedId,
    Duplicate(EntityId),
    Missing(EntityId),
    MissingPrefab(PrefabId),
    State(StateError),
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum MechanicsCommitError {
    Entity(EntityError),
    World(WorldCommitError),
}

impl From<StateError> for EntityError {
    fn from(value: StateError) -> Self {
        Self::State(value)
    }
}

/// Commit entity-directed commands in tape order. Failure restores the complete entity table.
pub fn commit_entities<const COMMANDS: usize, const ENTITIES: usize>(
    document: &MechanicsDocument,
    entities: &mut EntityTable<ENTITIES>,
    commands: &CommandTape<COMMANDS>,
) -> Result<(), EntityError> {
    let before = *entities;
    let result = commit_entities_inner(document, entities, commands);
    if result.is_err() {
        *entities = before;
    }
    result
}

fn commit_entities_inner<const COMMANDS: usize, const ENTITIES: usize>(
    document: &MechanicsDocument,
    entities: &mut EntityTable<ENTITIES>,
    commands: &CommandTape<COMMANDS>,
) -> Result<(), EntityError> {
    for &command in commands.as_slice() {
        match command {
            Command::Write { target, .. }
            | Command::Clamp { target, .. }
            | Command::Transition { target, .. } => {
                let entity = entities
                    .get_mut(target)
                    .ok_or(EntityError::Missing(target))?;
                let mut one = CommandTape::<1>::EMPTY;
                one.push(command)
                    .expect("a one-command tape always has room for one command");
                commit_instance(target, &mut entity.script, &one)?;
            }
            Command::Recompose { target, prefab } => {
                entities.recompose(document, target, prefab)?;
            }
            Command::Despawn(target) => entities.despawn(target)?,
            _ => {}
        }
    }
    Ok(())
}

/// Atomically commit one planned event across every rollback table currently owned by the
/// mechanics kernel. Adapters may consume unhandled physics/render commands after this succeeds.
pub fn commit_mechanics<
    const COMMANDS: usize,
    const ENTITIES: usize,
    const RELATIONS: usize,
    const EVENTS: usize,
>(
    document: &MechanicsDocument,
    entities: &mut EntityTable<ENTITIES>,
    world: &mut MechanicsWorld<RELATIONS>,
    commands: &CommandTape<COMMANDS>,
    emitted: &mut EventTape<EVENTS>,
) -> Result<(), MechanicsCommitError> {
    let entities_before = *entities;
    let world_before = *world;
    let emitted_before = *emitted;
    if let Err(error) = commit_entities(document, entities, commands) {
        return Err(MechanicsCommitError::Entity(error));
    }
    if let Err(error) = commit_world(world, commands, emitted) {
        *entities = entities_before;
        *world = world_before;
        *emitted = emitted_before;
        return Err(MechanicsCommitError::World(error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::mechanics::{Event, EventId, KindId, Program, ProgramDefinition};

    fn document() -> MechanicsDocument {
        MechanicsDocument {
            programs: vec![ProgramDefinition {
                id: ProgramId(1),
                program: Program::default(),
            }],
            prefabs: vec![PrefabDefinition {
                id: PrefabId(2),
                kind: KindId(3),
                program: ProgramId(1),
                initial: ScriptState::EMPTY,
            }],
            ..MechanicsDocument::default()
        }
    }

    #[test]
    fn recompose_keeps_id_and_resets_prefab_local_state() {
        let document = document();
        let mut entities = EntityTable::<1>::EMPTY;
        entities.spawn(&document, EntityId(9), PrefabId(2)).unwrap();
        entities.get_mut(EntityId(9)).unwrap().script.ints[0] = 99;
        entities
            .recompose(&document, EntityId(9), PrefabId(2))
            .unwrap();
        let entity = entities.get(EntityId(9)).unwrap();
        assert_eq!(entity.id, EntityId(9));
        assert_eq!(entity.script.ints[0], 0);
    }

    #[test]
    fn mechanics_commit_is_atomic_across_entity_and_event_tables() {
        let document = document();
        let mut entities = EntityTable::<1>::EMPTY;
        entities.spawn(&document, EntityId(9), PrefabId(2)).unwrap();
        entities.get_mut(EntityId(9)).unwrap().script.ints[0] = 99;
        let entities_before = entities;
        let mut world = MechanicsWorld::<0>::new();
        let world_before = world;
        let mut emitted = EventTape::<0>::EMPTY;
        let mut commands = CommandTape::<2>::EMPTY;
        commands
            .push(Command::Recompose {
                target: EntityId(9),
                prefab: PrefabId(2),
            })
            .unwrap();
        commands
            .push(Command::Emit(Event::new(
                EventId(7),
                EntityId(9),
                EntityId(9),
            )))
            .unwrap();

        assert_eq!(
            commit_mechanics(
                &document,
                &mut entities,
                &mut world,
                &commands,
                &mut emitted,
            ),
            Err(MechanicsCommitError::World(WorldCommitError::Events(
                super::super::EventTapeError::Full,
            )))
        );
        assert_eq!(entities, entities_before);
        assert_eq!(world, world_before);
        assert!(emitted.as_slice().is_empty());
    }
}
