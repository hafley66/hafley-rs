//! Serializable source-action inputs for the staged mutation boundary.
//!
//! This module records requests only. It does not read source bytes, group
//! edits, decide conflicts, derive output bytes, or mutate a filesystem. Those
//! operations need one complete action batch and belong to the stage planner.

use serde::{Deserialize, Serialize};

use crate::{
    ContentId, DirectoryId, FileRef, RepoPath, RepositoryId, RevisionId, RootPath, SourceRef,
    SourceSpan, WorktreeId,
};

/// The JSON schema version emitted for [`StageRequest`].
///
/// A consumer must reject a request whose `schema_version` is not this value
/// before interpreting its actions. Future incompatible request shapes receive
/// a new version instead of relying on serde defaults.
pub const SOURCE_ACTION_SCHEMA_VERSION: u32 = 1;

/// The mutable source root to which a [`StageRequest`] is confined.
///
/// A plain directory is identified only by its canonical [`DirectoryId`]. A
/// Git target is identified by both its shared [`RepositoryId`] and its exact
/// [`WorktreeId`], so two linked checkouts cannot accept each other's action
/// requests.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceRootId {
    Directory {
        directory: DirectoryId,
    },
    GitWorktree {
        repository: RepositoryId,
        worktree: WorktreeId,
    },
}

/// A root-relative destination path whose identity kind matches a
/// [`SourceRootId`].
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourcePath {
    Directory { path: RootPath },
    Git { path: RepoPath },
}

/// A revision-qualified existing source that can be an action target.
///
/// Git targets retain the complete [`SourceRef`] supplied by a producer. A
/// stage request accepts only `RevisionId::Worktree` targets matching its root;
/// immutable commit sources remain readable inputs and cannot be mutation
/// targets.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionSource {
    Directory { file: FileRef },
    Git { source: SourceRef },
}

/// A half-open byte range `[start, end)` within an [`ActionSource`].
///
/// This range remains generic over plain-directory and Git worktree targets.
/// [`From<SourceSpan>`] retains the existing Git span adapter without making
/// UTF-8 producers depend on a Git source identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ActionSpan {
    pub source: ActionSource,
    pub start: u64,
    pub end: u64,
}

impl From<SourceSpan> for ActionSpan {
    fn from(span: SourceSpan) -> Self {
        Self {
            source: ActionSource::Git {
                source: span.source,
            },
            start: span.start,
            end: span.end,
        }
    }
}

/// One producer's stable identity and optional ordering at a shared insertion
/// offset.
///
/// `same_offset_order` is considered only for multiple zero-width edits at the
/// same original-file byte offset. The stage planner requires every competing
/// insertion to carry a distinct explicit value before it chooses an order.
/// It never infers that order from `id` or input traversal order.
/// `rule` retains the rule or check identifier that emitted the edit and is
/// omitted from JSON when absent for compatibility with schema version 1.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ActionProducer {
    pub id: String,
    pub same_offset_order: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
}

impl ActionProducer {
    /// Construct a producer with no ordering claim at a shared insertion
    /// offset.
    pub fn unordered(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            same_offset_order: None,
            rule: None,
        }
    }

    /// Construct a producer with an explicit order at a shared insertion
    /// offset.
    pub fn ordered(id: impl Into<String>, same_offset_order: u64) -> Self {
        Self {
            id: id.into(),
            same_offset_order: Some(same_offset_order),
            rule: None,
        }
    }

    /// Attach the rule or check identifier that emitted this producer output.
    pub fn with_rule(mut self, rule: impl Into<String>) -> Self {
        self.rule = Some(rule.into());
        self
    }
}

/// A byte-oriented replacement against original-file byte offsets.
///
/// The range may split a UTF-8 code point because generated and binary source
/// files remain valid source-action inputs. [`Utf8TextEdit`] is the explicit
/// adapter boundary for producers whose replacement is guaranteed UTF-8.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TextEdit {
    pub range: ActionSpan,
    pub replacement: Vec<u8>,
    pub producer: ActionProducer,
}

impl TextEdit {
    /// Construct a byte edit from an existing Git [`SourceSpan`].
    pub fn from_source_span(
        range: SourceSpan,
        replacement: Vec<u8>,
        producer: ActionProducer,
    ) -> Self {
        Self {
            range: range.into(),
            replacement,
            producer,
        }
    }
}

/// A UTF-8 producer-side edit. Conversion to [`TextEdit`] encodes its
/// replacement as UTF-8 bytes without changing its original byte offsets or
/// producer metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Utf8TextEdit {
    pub range: ActionSpan,
    pub replacement: String,
    pub producer: ActionProducer,
}

impl From<Utf8TextEdit> for TextEdit {
    fn from(edit: Utf8TextEdit) -> Self {
        Self {
            range: edit.range,
            replacement: edit.replacement.into_bytes(),
            producer: edit.producer,
        }
    }
}

/// One requested filesystem action. `expected` is an optimistic content
/// precondition: the stage planner compares it with one observed input before
/// it derives an output, and the commit engine compares it again before writes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceAction {
    Create {
        path: SourcePath,
        bytes: Vec<u8>,
    },
    Replace {
        source: ActionSource,
        expected: ContentId,
        edits: Vec<TextEdit>,
    },
    Move {
        source: ActionSource,
        expected: ContentId,
        destination: SourcePath,
    },
    Delete {
        source: ActionSource,
        expected: ContentId,
    },
}

/// A versioned batch of source-action requests directed to one mutable root.
///
/// `actions` preserves producer submission order. Planner normalization has a
/// separate canonical order and must not overwrite this request record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageRequest {
    pub schema_version: u32,
    pub root: SourceRootId,
    pub actions: Vec<SourceAction>,
}

impl StageRequest {
    /// Create a request for the current source-action schema.
    pub fn new(root: SourceRootId, actions: Vec<SourceAction>) -> Self {
        Self {
            schema_version: SOURCE_ACTION_SCHEMA_VERSION,
            root,
            actions,
        }
    }

    /// Validate request-local identity and range shape without reading source
    /// bytes or comparing actions with one another.
    ///
    /// Cross-action checks, including duplicate insertion ordering, adjacency,
    /// overlap, source content freshness, and path collisions, require the
    /// stage planner's complete snapshot and are intentionally absent here.
    pub fn validate_shape(&self) -> Result<(), Vec<SourceActionValidationError>> {
        let mut errors = Vec::new();
        if self.schema_version != SOURCE_ACTION_SCHEMA_VERSION {
            errors.push(SourceActionValidationError::UnsupportedSchemaVersion {
                found: self.schema_version,
            });
        }
        for (action_index, action) in self.actions.iter().enumerate() {
            validate_action(&self.root, action_index, action, &mut errors);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// A deterministic request-local validation failure returned by
/// [`StageRequest::validate_shape`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceActionValidationError {
    UnsupportedSchemaVersion {
        found: u32,
    },
    PathRootMismatch {
        action_index: usize,
        path: SourcePath,
        root: SourceRootId,
    },
    SourceRootMismatch {
        action_index: usize,
        source: ActionSource,
        root: SourceRootId,
    },
    ImmutableGitSource {
        action_index: usize,
        source: SourceRef,
    },
    EditSourceMismatch {
        action_index: usize,
        edit_index: usize,
        action_source: ActionSource,
        edit_source: ActionSource,
    },
    InvertedEditRange {
        action_index: usize,
        edit_index: usize,
        start: u64,
        end: u64,
    },
}

fn validate_action(
    root: &SourceRootId,
    action_index: usize,
    action: &SourceAction,
    errors: &mut Vec<SourceActionValidationError>,
) {
    match action {
        SourceAction::Create { path, .. } => validate_path(root, action_index, path, errors),
        SourceAction::Replace { source, edits, .. } => {
            validate_source(root, action_index, source, errors);
            for (edit_index, edit) in edits.iter().enumerate() {
                if &edit.range.source != source {
                    errors.push(SourceActionValidationError::EditSourceMismatch {
                        action_index,
                        edit_index,
                        action_source: source.clone(),
                        edit_source: edit.range.source.clone(),
                    });
                }
                if edit.range.start > edit.range.end {
                    errors.push(SourceActionValidationError::InvertedEditRange {
                        action_index,
                        edit_index,
                        start: edit.range.start,
                        end: edit.range.end,
                    });
                }
            }
        }
        SourceAction::Move {
            source,
            destination,
            ..
        } => {
            validate_source(root, action_index, source, errors);
            validate_path(root, action_index, destination, errors);
        }
        SourceAction::Delete { source, .. } => validate_source(root, action_index, source, errors),
    }
}

fn validate_path(
    root: &SourceRootId,
    action_index: usize,
    path: &SourcePath,
    errors: &mut Vec<SourceActionValidationError>,
) {
    let compatible = matches!(
        (root, path),
        (SourceRootId::Directory { .. }, SourcePath::Directory { .. })
            | (SourceRootId::GitWorktree { .. }, SourcePath::Git { .. })
    );
    if !compatible {
        errors.push(SourceActionValidationError::PathRootMismatch {
            action_index,
            path: path.clone(),
            root: root.clone(),
        });
    }
}

fn validate_source(
    root: &SourceRootId,
    action_index: usize,
    source: &ActionSource,
    errors: &mut Vec<SourceActionValidationError>,
) {
    let compatible = match (root, source) {
        (
            SourceRootId::Directory { directory },
            ActionSource::Directory {
                file:
                    FileRef {
                        directory: source_directory,
                        ..
                    },
            },
        ) => directory == source_directory,
        (
            SourceRootId::GitWorktree {
                repository,
                worktree,
            },
            ActionSource::Git { source },
        ) => {
            if source.repository != *repository {
                false
            } else if let RevisionId::Worktree {
                worktree: source_worktree,
                ..
            } = &source.revision
            {
                source_worktree == worktree
            } else {
                errors.push(SourceActionValidationError::ImmutableGitSource {
                    action_index,
                    source: source.clone(),
                });
                return;
            }
        }
        _ => false,
    };
    if !compatible {
        errors.push(SourceActionValidationError::SourceRootMismatch {
            action_index,
            source: source.clone(),
            root: root.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn utf8_conversion_preserves_original_byte_offsets() {
        let edit = Utf8TextEdit {
            range: SourceSpan {
                source: SourceRef {
                    repository: RepositoryId(Arc::from("repository")),
                    revision: RevisionId::Worktree {
                        worktree: WorktreeId(Arc::from("worktree")),
                        head: None,
                        dirty: false,
                    },
                    path: RepoPath(Arc::from("src/lib.rs")),
                },
                start: 2,
                end: 5,
            }
            .into(),
            replacement: "é".into(),
            producer: ActionProducer::ordered("adapter", 3),
        };
        let bytes: TextEdit = edit.into();
        assert_eq!(bytes.range.start, 2);
        assert_eq!(bytes.range.end, 5);
        assert_eq!(bytes.replacement, "é".as_bytes());
        assert_eq!(bytes.producer.same_offset_order, Some(3));
    }
}
