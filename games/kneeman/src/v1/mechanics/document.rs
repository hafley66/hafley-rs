//! Static, hot-loadable mechanics content.
//!
//! A document is not rollback state. It is immutable authored data addressed by compact IDs;
//! rollback snapshots retain only entity records and world facts that can change per frame.

use super::{
    EntityId, Event, Field, KindId, PrefabId, Program, ProgramError, ProgramId, Relation,
    RuleSetId, ScriptState, Value,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ProgramDefinition {
    pub id: ProgramId,
    pub program: Program,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct PrefabDefinition {
    pub id: PrefabId,
    pub kind: KindId,
    pub program: ProgramId,
    #[serde(default)]
    pub initial: ScriptState,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct EntitySpawn {
    pub id: EntityId,
    pub prefab: PrefabId,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct DocumentStep {
    /// Entity whose current prefab/program receives this event.
    pub entity: EntityId,
    pub event: Event,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum DocumentExpectation {
    EntityPrefab {
        entity: EntityId,
        prefab: PrefabId,
    },
    EntityKind {
        entity: EntityId,
        kind: KindId,
    },
    EntityProgram {
        entity: EntityId,
        program: ProgramId,
    },
    FieldEquals {
        entity: EntityId,
        field: Field,
        value: Value,
    },
    Connected(Relation),
}

/// Portable semantic test shipped beside authored mechanics. No closures or Rust-only identity.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct DocumentCase {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub entities: Vec<EntitySpawn>,
    #[serde(default)]
    pub active_sets: Vec<RuleSetId>,
    #[serde(default)]
    pub relations: Vec<Relation>,
    #[serde(default)]
    pub steps: Vec<DocumentStep>,
    #[serde(default)]
    pub expect: Vec<DocumentExpectation>,
}

/// One or many authoring files normalize into this closed runtime document.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct MechanicsDocument {
    #[serde(default)]
    pub rule_sets: Vec<RuleSetId>,
    #[serde(default)]
    pub programs: Vec<ProgramDefinition>,
    #[serde(default)]
    pub prefabs: Vec<PrefabDefinition>,
    #[serde(default)]
    pub cases: Vec<DocumentCase>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum DocumentError {
    DuplicateRuleSet(RuleSetId),
    DuplicateProgram(ProgramId),
    DuplicatePrefab(PrefabId),
    InvalidProgram {
        program: ProgramId,
        errors: Vec<ProgramError>,
    },
    MissingProgram {
        prefab: PrefabId,
        program: ProgramId,
    },
    MissingCasePrefab {
        case: usize,
        prefab: PrefabId,
    },
    ReservedEntityId {
        case: usize,
    },
}

impl MechanicsDocument {
    pub fn program(&self, id: ProgramId) -> Option<&Program> {
        self.programs
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| &entry.program)
    }

    pub fn prefab(&self, id: PrefabId) -> Option<&PrefabDefinition> {
        self.prefabs.iter().find(|entry| entry.id == id)
    }

    pub fn validate(&self) -> Vec<DocumentError> {
        let mut errors = Vec::new();
        for (index, &id) in self.rule_sets.iter().enumerate() {
            if self.rule_sets[..index].contains(&id) {
                errors.push(DocumentError::DuplicateRuleSet(id));
            }
        }
        for (index, entry) in self.programs.iter().enumerate() {
            if self.programs[..index]
                .iter()
                .any(|seen| seen.id == entry.id)
            {
                errors.push(DocumentError::DuplicateProgram(entry.id));
            }
            let program_errors = entry.program.validate();
            if !program_errors.is_empty() {
                errors.push(DocumentError::InvalidProgram {
                    program: entry.id,
                    errors: program_errors,
                });
            }
        }
        for (index, prefab) in self.prefabs.iter().enumerate() {
            if self.prefabs[..index]
                .iter()
                .any(|seen| seen.id == prefab.id)
            {
                errors.push(DocumentError::DuplicatePrefab(prefab.id));
            }
            if self.program(prefab.program).is_none() {
                errors.push(DocumentError::MissingProgram {
                    prefab: prefab.id,
                    program: prefab.program,
                });
            }
        }
        for (case_index, case) in self.cases.iter().enumerate() {
            for spawn in &case.entities {
                if spawn.id.is_none() {
                    errors.push(DocumentError::ReservedEntityId { case: case_index });
                }
                if self.prefab(spawn.prefab).is_none() {
                    errors.push(DocumentError::MissingCasePrefab {
                        case: case_index,
                        prefab: spawn.prefab,
                    });
                }
            }
        }
        errors
    }
}
