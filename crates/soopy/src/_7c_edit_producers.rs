//! Producer-facing edit adapters.
//!
//! This module converts edit records into the source-action range and byte
//! representation. It does not parse source, inspect a filesystem, apply
//! edits, or resolve conflicts. A deduplicated edit retains every producer
//! that emitted the same range and replacement; conflict decisions stay with
//! the stage planner.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{ActionProducer, ActionSource, ActionSpan, TextEdit, Utf8TextEdit};

/// Schema version for producer edit envelopes and batches.
pub const PRODUCED_EDIT_SCHEMA_VERSION: u32 = 1;

/// One normalized edit emitted by a structural or text edit producer.
///
/// The range is in original-file byte coordinates. `producers` normally has
/// one entry. Deduplication merges equivalent outputs and retains all entries
/// so the planner can make a policy decision with complete attribution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProducedEdit {
    pub range: ActionSpan,
    pub replacement: Vec<u8>,
    pub producers: Vec<ActionProducer>,
}

impl ProducedEdit {
    pub fn new(
        range: ActionSpan,
        replacement: impl Into<Vec<u8>>,
        producer: ActionProducer,
    ) -> Self {
        Self {
            range,
            replacement: replacement.into(),
            producers: vec![producer],
        }
    }

    /// Wrap the existing byte-oriented action shape without changing it.
    pub fn from_text_edit(edit: TextEdit) -> Self {
        Self {
            range: edit.range,
            replacement: edit.replacement,
            producers: vec![edit.producer],
        }
    }

    /// Wrap the existing UTF-8 action shape. Conversion to bytes is UTF-8
    /// encoding; source offsets and producer metadata are retained.
    pub fn from_utf8_text_edit(edit: Utf8TextEdit) -> Self {
        Self::from_text_edit(edit.into())
    }
}

/// Lossless planner input. The planner receives every non-equivalent output,
/// including overlapping or otherwise conflicting edits, with provenance
/// intact. Conflict decisions remain planner-owned.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProducedEditBatch {
    pub schema_version: u32,
    pub edits: Vec<ProducedEdit>,
}

impl ProducedEditBatch {
    pub fn new(edits: Vec<ProducedEdit>) -> Self {
        Self {
            schema_version: PRODUCED_EDIT_SCHEMA_VERSION,
            edits,
        }
    }

    pub fn validate(&self) -> Result<(), Vec<ProducedEditBatchValidationError>> {
        let mut errors = Vec::new();
        if self.schema_version != PRODUCED_EDIT_SCHEMA_VERSION {
            errors.push(ProducedEditBatchValidationError::UnsupportedSchemaVersion {
                found: self.schema_version,
            });
        }
        for (edit_index, edit) in self.edits.iter().enumerate() {
            if edit.producers.is_empty() {
                errors.push(ProducedEditBatchValidationError::EmptyProducers { edit_index });
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Expand one produced edit into one `TextEdit` per producer. This is the
    /// lossless bridge into the existing action algebra: equivalent outputs
    /// retain all rule-bearing producers, while non-equivalent outputs remain
    /// separate for planner conflict detection.
    pub fn into_text_edits(self) -> Result<Vec<TextEdit>, Vec<ProducedEditBatchValidationError>> {
        self.validate()?;
        Ok(self
            .edits
            .into_iter()
            .flat_map(|edit| {
                edit.producers.into_iter().map(move |producer| TextEdit {
                    range: edit.range.clone(),
                    replacement: edit.replacement.clone(),
                    producer,
                })
            })
            .collect())
    }

    pub fn deduplicate(self) -> Result<Self, Vec<ProducedEditBatchValidationError>> {
        self.validate()?;
        Ok(Self::new(deduplicate_equivalent_edits(self.edits)))
    }

    pub fn into_edits(self) -> Result<Vec<ProducedEdit>, Vec<ProducedEditBatchValidationError>> {
        self.validate()?;
        Ok(self.edits)
    }
}

/// Deterministic producer batch validation failures.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProducedEditBatchValidationError {
    UnsupportedSchemaVersion { found: u32 },
    EmptyProducers { edit_index: usize },
}

/// Group byte-identical edits by original range and retain every distinct
/// producer. The returned order is canonical by source range and replacement
/// bytes. Overlaps and non-equivalent edits are intentionally left untouched
/// for planner conflict handling.
pub fn deduplicate_equivalent_edits(
    edits: impl IntoIterator<Item = ProducedEdit>,
) -> Vec<ProducedEdit> {
    let mut groups: BTreeMap<(ActionSpan, Vec<u8>), Vec<ActionProducer>> = BTreeMap::new();
    for edit in edits {
        groups
            .entry((edit.range, edit.replacement))
            .or_default()
            .extend(edit.producers);
    }
    groups
        .into_iter()
        .map(|((range, replacement), mut producers)| {
            producers.sort();
            producers.dedup();
            ProducedEdit {
                range,
                replacement,
                producers,
            }
        })
        .collect()
}

/// The stable, dependency-free contract used when Biome's language crates are
/// unavailable. It describes the public mutation payload needed by Soopy and
/// is not a Biome runtime type. Serialize this record as a fixture at the
/// Biome integration boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BiomeBatchMutationContract {
    pub range: ActionSpan,
    pub replacement: String,
    pub producer: ActionProducer,
}

impl From<BiomeBatchMutationContract> for ProducedEdit {
    fn from(edit: BiomeBatchMutationContract) -> Self {
        Self::new(edit.range, edit.replacement.into_bytes(), edit.producer)
    }
}

/// Adapt the public scalar fields of ast-grep's `source::Edit<S>` record.
///
/// The caller extracts `position`, `deleted_length`, and `inserted_text` from
/// the real ast-grep value. Soopy does not recreate that foreign output type,
/// depend on a parser/runtime crate, or apply the edit.
pub fn from_ast_grep_parts(
    source: ActionSource,
    position: usize,
    deleted_length: usize,
    inserted_bytes: impl Into<Vec<u8>>,
    producer: ActionProducer,
) -> Result<ProducedEdit, ProducedEditConversionError> {
    let start =
        u64::try_from(position).map_err(|_| ProducedEditConversionError::AstGrepOffsetOverflow)?;
    let deleted = u64::try_from(deleted_length)
        .map_err(|_| ProducedEditConversionError::AstGrepOffsetOverflow)?;
    let end = start
        .checked_add(deleted)
        .ok_or(ProducedEditConversionError::AstGrepOffsetOverflow)?;
    Ok(ProducedEdit::new(
        ActionSpan { source, start, end },
        inserted_bytes,
        producer,
    ))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProducedEditConversionError {
    AstGrepOffsetOverflow,
}
