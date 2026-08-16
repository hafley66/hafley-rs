//! Deterministic, read-only normalization for staged source mutations.
//!
//! The planner verifies every existing source once, derives replacement bytes
//! in memory, and returns a canonical file plan. It deliberately contains no
//! target filesystem, Git index, ref, or commit mutation.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    ActionProducer, ActionSource, ContentId, FileReadRequest, ReadRequest, SourceAction,
    SourceActionValidationError, SourcePath, SourceRoot, SourceRootId, StageRequest, TextEdit,
};

/// A sealed-in-memory result of normalizing a [`StageRequest`].
///
/// `files` has canonical path order and contains every materialized output
/// identity. The bytes remain in memory for a later staging-store boundary;
/// this module does not persist them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationPlan {
    pub root: SourceRootId,
    pub files: Vec<PlannedFile>,
}

/// One source file outcome in a [`MutationPlan`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedFile {
    pub kind: PlannedFileKind,
    pub source: Option<ActionSource>,
    pub path_before: Option<SourcePath>,
    pub path_after: Option<SourcePath>,
    pub content_before: Option<ContentId>,
    pub content_after: Option<ContentId>,
    pub mode_before: Option<FileModeObservation>,
    pub bytes_before: Option<Vec<u8>>,
    pub bytes_after: Option<Vec<u8>>,
    pub edits: Vec<NormalizedEdit>,
}

/// Existing target metadata retained for the commit phase. The source action
/// stage does not change modes; commit 022 uses this observation when replacing
/// or moving a file so permissions remain stable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileModeObservation {
    pub readonly: bool,
    pub unix_mode: Option<u32>,
}

/// The normalized action represented by a [`PlannedFile`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannedFileKind {
    Create,
    Replace,
    Move,
    Delete,
}

/// One accepted original-coordinate edit and all producers that supplied its
/// equivalent byte replacement. Producers are deduplicated and sorted by their
/// structural identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedEdit {
    pub start: u64,
    pub end: u64,
    pub replacement: Vec<u8>,
    pub producers: Vec<ActionProducer>,
}

/// Validate, read, normalize, and materialize a complete in-memory plan.
///
/// Grouping uses one [`ActionSource`] map key per distinct existing target.
/// The directory branch constructs one [`FileReadRequest`] per key and passes
/// that vector to one `DirectoryRoot::read_each` call. The Git branch does the
/// equivalent with one [`ReadRequest`] per key and one `SourceTree::read_each`
/// call. Neither branch issues another target byte read during normalization.
/// A stale input returns [`StageRefusal::Stale`] before this function produces
/// any [`MutationPlan`].
pub fn plan_mutations(
    root: &mut SourceRoot,
    request: &StageRequest,
) -> Result<MutationPlan, StageRefusal> {
    let actual_root = root_identity(root);
    if request.root != actual_root {
        return Err(StageRefusal::RootMismatch {
            requested: request.root.clone(),
            actual: actual_root,
        });
    }
    if let Err(errors) = request.validate_shape() {
        return Err(StageRefusal::InvalidRequest { errors });
    }

    let invalid_paths = invalid_paths(request);
    if !invalid_paths.is_empty() {
        return Err(StageRefusal::InvalidPath {
            paths: invalid_paths,
        });
    }

    let grouped = group_actions(root, request)?;
    let inputs = read_inputs(root, &grouped)?;
    let stale = stale_inputs(&grouped, &inputs);
    if !stale.is_empty() {
        return Err(StageRefusal::Stale { inputs: stale });
    }

    let mut files = Vec::with_capacity(grouped.sources.len() + grouped.creates.len());
    for (path, bytes) in grouped.creates {
        files.push(PlannedFile {
            kind: PlannedFileKind::Create,
            source: None,
            path_before: None,
            path_after: Some(path),
            content_before: None,
            content_after: Some(blake3_content(&bytes)),
            mode_before: None,
            bytes_before: None,
            bytes_after: Some(bytes),
            edits: Vec::new(),
        });
    }
    for (source, group) in grouped.sources {
        let input = inputs
            .get(&source)
            .expect("every normalized source was read exactly once");
        match group.operation {
            SourceOperation::Replace { edits, .. } => {
                let edits = normalize_edits(&source, edits, input.bytes.len())?;
                let bytes_after = apply_edits(&input.bytes, &edits)?;
                files.push(PlannedFile {
                    kind: PlannedFileKind::Replace,
                    source: Some(source.clone()),
                    path_before: Some(source_path(&source)),
                    path_after: Some(source_path(&source)),
                    content_before: Some(input.content.clone()),
                    content_after: Some(blake3_content(&bytes_after)),
                    mode_before: Some(input.mode.clone()),
                    bytes_before: Some(input.bytes.clone()),
                    bytes_after: Some(bytes_after),
                    edits,
                });
            }
            SourceOperation::Move { destination, .. } => files.push(PlannedFile {
                kind: PlannedFileKind::Move,
                source: Some(source.clone()),
                path_before: Some(source_path(&source)),
                path_after: Some(destination),
                content_before: Some(input.content.clone()),
                content_after: Some(input.content.clone()),
                mode_before: Some(input.mode.clone()),
                bytes_before: Some(input.bytes.clone()),
                bytes_after: Some(input.bytes.clone()),
                edits: Vec::new(),
            }),
            SourceOperation::Delete { .. } => files.push(PlannedFile {
                kind: PlannedFileKind::Delete,
                source: Some(source.clone()),
                path_before: Some(source_path(&source)),
                path_after: None,
                content_before: Some(input.content.clone()),
                content_after: None,
                mode_before: Some(input.mode.clone()),
                bytes_before: Some(input.bytes.clone()),
                bytes_after: None,
                edits: Vec::new(),
            }),
        }
    }
    files.sort_by_key(planned_file_key);
    Ok(MutationPlan {
        root: request.root.clone(),
        files,
    })
}

/// A typed refusal from [`plan_mutations`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StageRefusal {
    RootMismatch {
        requested: SourceRootId,
        actual: SourceRootId,
    },
    InvalidRequest {
        errors: Vec<SourceActionValidationError>,
    },
    InvalidPath {
        paths: Vec<InvalidMutationPath>,
    },
    Conflict {
        conflicts: Vec<MutationConflict>,
    },
    Stale {
        inputs: Vec<StaleInput>,
    },
    Unreadable {
        source: ActionSource,
        detail: String,
    },
    Store {
        detail: String,
    },
}

impl std::fmt::Display for StageRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "source mutation stage refused: {self:?}")
    }
}

impl std::error::Error for StageRefusal {}

/// A root-relative path rejected before it is joined to a host filesystem path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvalidMutationPath {
    pub action_index: usize,
    pub path: SourcePath,
    pub reason: InvalidPathReason,
}

/// The structural reason a path cannot be a mutable target or destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvalidPathReason {
    Empty,
    Absolute,
    Traversal,
    RecordSeparator,
}

/// A batch-wide deterministic normalization conflict.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MutationConflict {
    ExpectedContentMismatch {
        source: ActionSource,
        expected: Vec<ContentId>,
    },
    SourceOperationCollision {
        source: ActionSource,
    },
    PathCollision {
        path: SourcePath,
    },
    OccupiedDestination {
        path: SourcePath,
    },
    EditOutOfBounds {
        source: ActionSource,
        start: u64,
        end: u64,
        input_len: u64,
    },
    OverlappingEdits {
        source: ActionSource,
        left: NormalizedEdit,
        right: NormalizedEdit,
    },
    SameOffsetOrderRequired {
        source: ActionSource,
        offset: u64,
        edits: Vec<NormalizedEdit>,
    },
    SameOffsetOrderCollision {
        source: ActionSource,
        offset: u64,
        edits: Vec<NormalizedEdit>,
    },
}

/// One source whose observed bytes did not satisfy its optimistic precondition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaleInput {
    pub source: ActionSource,
    pub expected: ContentId,
    pub observed: Option<ContentId>,
}

#[derive(Clone)]
struct SourceInput {
    content: ContentId,
    bytes: Vec<u8>,
    mode: FileModeObservation,
}

struct GroupedActions {
    creates: BTreeMap<SourcePath, Vec<u8>>,
    sources: BTreeMap<ActionSource, SourceActionGroup>,
}

struct SourceActionGroup {
    operation: SourceOperation,
}

enum SourceOperation {
    Replace {
        expected: ContentId,
        edits: Vec<TextEdit>,
    },
    Move {
        expected: ContentId,
        destination: SourcePath,
    },
    Delete {
        expected: ContentId,
    },
}

fn root_identity(root: &SourceRoot) -> SourceRootId {
    match root {
        SourceRoot::Directory(directory) => SourceRootId::Directory {
            directory: directory.identity.clone(),
        },
        SourceRoot::GitWorktree(git) => SourceRootId::GitWorktree {
            repository: git.repository.identity.clone(),
            worktree: git.repository.worktree.clone(),
        },
    }
}

fn invalid_paths(request: &StageRequest) -> Vec<InvalidMutationPath> {
    let mut invalid = Vec::new();
    for (action_index, action) in request.actions.iter().enumerate() {
        match action {
            SourceAction::Create { path, .. } => {
                push_invalid_path(&mut invalid, action_index, path)
            }
            SourceAction::Replace { source, .. } | SourceAction::Delete { source, .. } => {
                push_invalid_path(&mut invalid, action_index, &source_path(source));
            }
            SourceAction::Move {
                source,
                destination,
                ..
            } => {
                push_invalid_path(&mut invalid, action_index, &source_path(source));
                push_invalid_path(&mut invalid, action_index, destination);
            }
        }
    }
    invalid
}

fn push_invalid_path(
    invalid: &mut Vec<InvalidMutationPath>,
    action_index: usize,
    path: &SourcePath,
) {
    let raw = path_text(path);
    let reason = if raw.is_empty() || raw == "." {
        Some(InvalidPathReason::Empty)
    } else if raw.contains('\n') || raw.contains('\r') {
        Some(InvalidPathReason::RecordSeparator)
    } else if std::path::Path::new(raw).is_absolute() {
        Some(InvalidPathReason::Absolute)
    } else if std::path::Path::new(raw)
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        Some(InvalidPathReason::Traversal)
    } else {
        None
    };
    if let Some(reason) = reason {
        invalid.push(InvalidMutationPath {
            action_index,
            path: path.clone(),
            reason,
        });
    }
}

fn group_actions(
    root: &SourceRoot,
    request: &StageRequest,
) -> Result<GroupedActions, StageRefusal> {
    let mut creates = BTreeMap::new();
    let mut sources = BTreeMap::<ActionSource, SourceActionGroup>::new();
    let mut conflicts = Vec::new();

    for action in &request.actions {
        match action {
            SourceAction::Create { path, bytes } => {
                if creates.insert(path.clone(), bytes.clone()).is_some() {
                    conflicts.push(MutationConflict::PathCollision { path: path.clone() });
                }
            }
            SourceAction::Replace {
                source,
                expected,
                edits,
            } => match sources.get_mut(source) {
                None => {
                    sources.insert(
                        source.clone(),
                        SourceActionGroup {
                            operation: SourceOperation::Replace {
                                expected: expected.clone(),
                                edits: edits.clone(),
                            },
                        },
                    );
                }
                Some(SourceActionGroup {
                    operation:
                        SourceOperation::Replace {
                            expected: existing,
                            edits: existing_edits,
                        },
                }) if existing == expected => existing_edits.extend(edits.clone()),
                Some(SourceActionGroup {
                    operation:
                        SourceOperation::Replace {
                            expected: existing, ..
                        },
                }) => conflicts.push(MutationConflict::ExpectedContentMismatch {
                    source: source.clone(),
                    expected: canonical_content_ids([existing.clone(), expected.clone()]),
                }),
                Some(_) => conflicts.push(MutationConflict::SourceOperationCollision {
                    source: source.clone(),
                }),
            },
            SourceAction::Move {
                source,
                expected,
                destination,
            } => insert_non_replace(
                &mut sources,
                source,
                SourceOperation::Move {
                    expected: expected.clone(),
                    destination: destination.clone(),
                },
                &mut conflicts,
            ),
            SourceAction::Delete { source, expected } => insert_non_replace(
                &mut sources,
                source,
                SourceOperation::Delete {
                    expected: expected.clone(),
                },
                &mut conflicts,
            ),
        }
    }

    let source_paths: BTreeSet<_> = sources.keys().map(source_path).collect();
    let mut destinations = BTreeSet::new();
    for path in creates.keys() {
        if !destinations.insert(path.clone()) {
            conflicts.push(MutationConflict::PathCollision { path: path.clone() });
        }
        if source_paths.contains(path) || destination_is_occupied(root, path) {
            conflicts.push(MutationConflict::OccupiedDestination { path: path.clone() });
        }
    }
    for group in sources.values() {
        if let SourceOperation::Move { destination, .. } = &group.operation {
            if !destinations.insert(destination.clone()) {
                conflicts.push(MutationConflict::PathCollision {
                    path: destination.clone(),
                });
            }
            if source_paths.contains(destination) || destination_is_occupied(root, destination) {
                conflicts.push(MutationConflict::OccupiedDestination {
                    path: destination.clone(),
                });
            }
        }
    }
    conflicts.sort_by(|left, right| format!("{left:?}").cmp(&format!("{right:?}")));
    conflicts.dedup();
    if conflicts.is_empty() {
        Ok(GroupedActions { creates, sources })
    } else {
        Err(StageRefusal::Conflict { conflicts })
    }
}

fn insert_non_replace(
    sources: &mut BTreeMap<ActionSource, SourceActionGroup>,
    source: &ActionSource,
    operation: SourceOperation,
    conflicts: &mut Vec<MutationConflict>,
) {
    if sources
        .insert(source.clone(), SourceActionGroup { operation })
        .is_some()
    {
        conflicts.push(MutationConflict::SourceOperationCollision {
            source: source.clone(),
        });
    }
}

fn destination_is_occupied(root: &SourceRoot, path: &SourcePath) -> bool {
    let (base, relative) = match (root, path) {
        (SourceRoot::Directory(directory), SourcePath::Directory { path }) => {
            (&directory.root, path.0.as_ref())
        }
        (SourceRoot::GitWorktree(git), SourcePath::Git { path }) => {
            (&git.repository.root, path.0.as_ref())
        }
        _ => return false,
    };
    std::fs::symlink_metadata(base.join(relative)).is_ok()
}

fn direct_source_mode(
    root: &SourceRoot,
    source: &ActionSource,
) -> Result<FileModeObservation, String> {
    let path = match (root, source) {
        (SourceRoot::Directory(directory), ActionSource::Directory { file }) => {
            directory.root.join(file.path.0.as_ref())
        }
        (SourceRoot::GitWorktree(git), ActionSource::Git { source }) => {
            git.repository.root.join(source.path.0.as_ref())
        }
        _ => return Err("source root does not match target root".to_owned()),
    };
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|error| format!("read direct source metadata {}: {error}", path.display()))?;
    if !metadata.file_type().is_file() {
        return Err(format!(
            "source target is not a regular file: {}",
            path.display()
        ));
    }
    #[cfg(unix)]
    let unix_mode = {
        use std::os::unix::fs::MetadataExt;
        Some(metadata.mode())
    };
    #[cfg(not(unix))]
    let unix_mode = None;
    Ok(FileModeObservation {
        readonly: metadata.permissions().readonly(),
        unix_mode,
    })
}

fn read_inputs(
    root: &mut SourceRoot,
    grouped: &GroupedActions,
) -> Result<BTreeMap<ActionSource, SourceInput>, StageRefusal> {
    let mut modes = BTreeMap::new();
    for source in grouped.sources.keys() {
        let mode = direct_source_mode(root, source).map_err(|detail| StageRefusal::Unreadable {
            source: source.clone(),
            detail,
        })?;
        modes.insert(source.clone(), mode);
    }
    match root {
        SourceRoot::Directory(directory) => {
            let requests: Vec<_> = grouped
                .sources
                .keys()
                .map(|source| match source {
                    ActionSource::Directory { file } => FileReadRequest {
                        file: file.clone(),
                        expected: None,
                    },
                    ActionSource::Git { .. } => unreachable!("request shape validated root kind"),
                })
                .collect();
            let mut inputs = BTreeMap::new();
            let result = directory.read_each(&requests, |answer| {
                inputs.insert(
                    ActionSource::Directory {
                        file: answer.file.clone(),
                    },
                    SourceInput {
                        content: answer.content.clone(),
                        bytes: answer.bytes.to_vec(),
                        mode: modes
                            .get(&ActionSource::Directory {
                                file: answer.file.clone(),
                            })
                            .expect("mode checked before read")
                            .clone(),
                    },
                );
                Ok(())
            });
            if let Err(error) = result {
                let source = grouped
                    .sources
                    .keys()
                    .find(|source| !inputs.contains_key(*source))
                    .expect("failed directory read has an unread request")
                    .clone();
                return Err(StageRefusal::Unreadable {
                    source,
                    detail: error.to_string(),
                });
            }
            Ok(inputs)
        }
        SourceRoot::GitWorktree(git) => {
            let requests: Vec<_> = grouped
                .sources
                .iter()
                .map(|(source, group)| match (source, &group.operation) {
                    (ActionSource::Git { source }, operation) => ReadRequest {
                        source: source.clone(),
                        expected: expected_for_git_read(operation),
                    },
                    (ActionSource::Directory { .. }, _) => {
                        unreachable!("request shape validated root kind")
                    }
                })
                .collect();
            let mut inputs = BTreeMap::new();
            let mut buffer = Vec::new();
            let result = git
                .source_tree()
                .read_each(&requests, &mut buffer, |answer| {
                    inputs.insert(
                        ActionSource::Git {
                            source: answer.source.clone(),
                        },
                        SourceInput {
                            content: answer.content.clone(),
                            bytes: answer.bytes.to_vec(),
                            mode: modes
                                .get(&ActionSource::Git {
                                    source: answer.source.clone(),
                                })
                                .expect("mode checked before read")
                                .clone(),
                        },
                    );
                    Ok(())
                });
            if let Err(error) = result {
                let source = requests
                    .iter()
                    .find(|request| {
                        !inputs.contains_key(&ActionSource::Git {
                            source: request.source.clone(),
                        })
                    })
                    .map(|request| ActionSource::Git {
                        source: request.source.clone(),
                    })
                    .expect("failed Git read has an unread request");
                if error.to_string().contains("content changed") {
                    let expected = expected_for_git_read(
                        &grouped
                            .sources
                            .get(&source)
                            .expect("source was grouped")
                            .operation,
                    )
                    .expect("only Git blob reads use backend freshness");
                    return Err(StageRefusal::Stale {
                        inputs: vec![StaleInput {
                            source,
                            expected,
                            observed: None,
                        }],
                    });
                }
                return Err(StageRefusal::Unreadable {
                    source,
                    detail: error.to_string(),
                });
            }
            Ok(inputs)
        }
    }
}

fn expected_for_git_read(operation: &SourceOperation) -> Option<ContentId> {
    let expected = match operation {
        SourceOperation::Replace { expected, .. }
        | SourceOperation::Move { expected, .. }
        | SourceOperation::Delete { expected } => expected,
    };
    matches!(expected, ContentId::GitBlob(_)).then(|| expected.clone())
}

fn stale_inputs(
    grouped: &GroupedActions,
    inputs: &BTreeMap<ActionSource, SourceInput>,
) -> Vec<StaleInput> {
    grouped
        .sources
        .iter()
        .filter_map(|(source, group)| {
            let expected = expected_content(&group.operation);
            let observed = &inputs
                .get(source)
                .expect("every normalized source was read exactly once")
                .content;
            (expected != observed).then(|| StaleInput {
                source: source.clone(),
                expected: expected.clone(),
                observed: Some(observed.clone()),
            })
        })
        .collect()
}

fn expected_content(operation: &SourceOperation) -> &ContentId {
    match operation {
        SourceOperation::Replace { expected, .. }
        | SourceOperation::Move { expected, .. }
        | SourceOperation::Delete { expected } => expected,
    }
}

fn canonical_content_ids(ids: impl IntoIterator<Item = ContentId>) -> Vec<ContentId> {
    let mut ids: Vec<_> = ids.into_iter().collect();
    ids.sort();
    ids.dedup();
    ids
}

fn normalize_edits(
    source: &ActionSource,
    edits: Vec<TextEdit>,
    input_len: usize,
) -> Result<Vec<NormalizedEdit>, StageRefusal> {
    let input_len = u64::try_from(input_len).expect("usize always fits u64");
    let mut merged = BTreeMap::<(u64, u64, Vec<u8>), BTreeSet<ActionProducer>>::new();
    for edit in edits {
        merged
            .entry((edit.range.start, edit.range.end, edit.replacement))
            .or_default()
            .insert(edit.producer);
    }
    if let Some(((start, end, _), _)) = merged
        .iter()
        .find(|((start, end, _), _)| *start > input_len || *end > input_len)
    {
        return Err(StageRefusal::Conflict {
            conflicts: vec![MutationConflict::EditOutOfBounds {
                source: source.clone(),
                start: *start,
                end: *end,
                input_len,
            }],
        });
    }
    let mut normalized: Vec<_> = merged
        .into_iter()
        .map(|((start, end, replacement), producers)| NormalizedEdit {
            start,
            end,
            replacement,
            producers: producers.into_iter().collect(),
        })
        .collect();

    let nonempty: Vec<_> = normalized
        .iter()
        .filter(|edit| edit.start < edit.end)
        .collect();
    for pair in nonempty.windows(2) {
        let [left, right] = pair else { unreachable!() };
        if left.end > right.start {
            return Err(StageRefusal::Conflict {
                conflicts: vec![MutationConflict::OverlappingEdits {
                    source: source.clone(),
                    left: (*left).clone(),
                    right: (*right).clone(),
                }],
            });
        }
    }
    for insertion in normalized.iter().filter(|edit| edit.start == edit.end) {
        if let Some(replacement) = nonempty
            .iter()
            .find(|edit| edit.start < insertion.start && insertion.start < edit.end)
        {
            return Err(StageRefusal::Conflict {
                conflicts: vec![MutationConflict::OverlappingEdits {
                    source: source.clone(),
                    left: (*replacement).clone(),
                    right: insertion.clone(),
                }],
            });
        }
    }

    let insertion_groups = normalized
        .iter()
        .filter(|edit| edit.start == edit.end)
        .fold(
            BTreeMap::<u64, Vec<&NormalizedEdit>>::new(),
            |mut groups, edit| {
                groups.entry(edit.start).or_default().push(edit);
                groups
            },
        );
    let mut orders = BTreeMap::new();
    for (offset, group) in insertion_groups {
        if group.len() < 2 {
            continue;
        }
        let mut claimed = BTreeSet::new();
        let mut missing = false;
        let mut collision = false;
        for edit in &group {
            let distinct: BTreeSet<_> = edit
                .producers
                .iter()
                .map(|producer| producer.same_offset_order)
                .collect();
            let producer_orders: Vec<_> = distinct.into_iter().collect();
            let [Some(order)] = producer_orders.as_slice() else {
                missing = true;
                continue;
            };
            if !claimed.insert(*order) {
                collision = true;
            }
            orders.insert((offset, edit.replacement.clone()), *order);
        }
        if missing {
            return Err(StageRefusal::Conflict {
                conflicts: vec![MutationConflict::SameOffsetOrderRequired {
                    source: source.clone(),
                    offset,
                    edits: group.into_iter().cloned().collect(),
                }],
            });
        }
        if collision {
            return Err(StageRefusal::Conflict {
                conflicts: vec![MutationConflict::SameOffsetOrderCollision {
                    source: source.clone(),
                    offset,
                    edits: group.into_iter().cloned().collect(),
                }],
            });
        }
    }
    normalized.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| {
                orders
                    .get(&(left.start, left.replacement.clone()))
                    .cmp(&orders.get(&(right.start, right.replacement.clone())))
            })
            .then_with(|| left.end.cmp(&right.end))
            .then_with(|| left.replacement.cmp(&right.replacement))
    });
    Ok(normalized)
}

fn apply_edits(input: &[u8], edits: &[NormalizedEdit]) -> Result<Vec<u8>, StageRefusal> {
    let mut output = input.to_vec();
    for edit in edits.iter().rev() {
        let start = usize::try_from(edit.start).expect("validated offset fits usize");
        let end = usize::try_from(edit.end).expect("validated offset fits usize");
        output.splice(start..end, edit.replacement.iter().copied());
    }
    Ok(output)
}

fn source_path(source: &ActionSource) -> SourcePath {
    match source {
        ActionSource::Directory { file } => SourcePath::Directory {
            path: file.path.clone(),
        },
        ActionSource::Git { source } => SourcePath::Git {
            path: source.path.clone(),
        },
    }
}

fn path_text(path: &SourcePath) -> &str {
    match path {
        SourcePath::Directory { path } => path.0.as_ref(),
        SourcePath::Git { path } => path.0.as_ref(),
    }
}

fn blake3_content(bytes: &[u8]) -> ContentId {
    ContentId::Blake3(*blake3::hash(bytes).as_bytes())
}

fn planned_file_key(
    file: &PlannedFile,
) -> (Option<SourcePath>, Option<SourcePath>, PlannedFileKind) {
    (file.path_after.clone(), file.path_before.clone(), file.kind)
}
