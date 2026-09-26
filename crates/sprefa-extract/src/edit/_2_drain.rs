//! soopy source actions: text edits folded into the ONE Replace soopy takes
//! per source file, plus the staging helpers the move/rename verbs share.

use std::path::Path;
use std::sync::Arc;

/// Byte edits fold into one Replace. Sorted and deduped by (start, end): two
/// producers on one span are one edit.
pub fn replace_action(
    source: soopy::ActionSource,
    expected: soopy::ContentId,
    mut edits: Vec<soopy::TextEdit>,
) -> soopy::SourceAction {
    edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
    edits.dedup_by_key(|edit| (edit.range.start, edit.range.end));
    soopy::SourceAction::Replace {
        source,
        expected,
        edits,
    }
}

/// The staged request for one file's drained edits.
pub fn stage_edits(
    source: soopy::ActionSource,
    expected: soopy::ContentId,
    edits: Vec<soopy::TextEdit>,
    root_id: soopy::SourceRootId,
) -> soopy::StageRequest {
    soopy::StageRequest::new(root_id, vec![replace_action(source, expected, edits)])
}

/// A root-relative file in a plain-directory source root.
pub fn directory_source(identity: &soopy::DirectoryId, rel: &str) -> soopy::ActionSource {
    soopy::ActionSource::Directory {
        file: soopy::FileRef {
            directory: identity.clone(),
            path: soopy::RootPath(Arc::from(rel)),
        },
    }
}

/// A root-relative destination path in a plain-directory source root.
pub fn directory_path(rel: &str) -> soopy::SourcePath {
    soopy::SourcePath::Directory {
        path: soopy::RootPath(Arc::from(rel)),
    }
}

/// The root-relative path an action reads from, or None for a Create.
pub fn source_rel(action: &soopy::SourceAction) -> Option<&str> {
    let source = match action {
        soopy::SourceAction::Create { .. } => return None,
        soopy::SourceAction::Replace { source, .. }
        | soopy::SourceAction::Move { source, .. }
        | soopy::SourceAction::Delete { source, .. } => source,
    };
    match source {
        soopy::ActionSource::Directory { file } => Some(&file.path.0),
        soopy::ActionSource::Git { .. } => None,
    }
}

/// Re-aim a planned action at the root staging it: `DirectoryId` is blake3 of
/// the canonical root path. A Replace keeps the `expected` its edits were cut
/// against; a Move re-reads, since an earlier stage may have edited it.
pub fn bind_action(
    root: &Path,
    identity: &soopy::DirectoryId,
    action: &soopy::SourceAction,
) -> Result<soopy::SourceAction, String> {
    let expected = |rel: &str| -> Result<soopy::ContentId, String> {
        let path = root.join(rel);
        let bytes =
            std::fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
        Ok(soopy::ContentId::blake3(&bytes))
    };
    let Some(rel) = source_rel(action) else {
        return Ok(action.clone());
    };
    let source = directory_source(identity, rel);
    Ok(match action {
        soopy::SourceAction::Create { .. } => action.clone(),
        soopy::SourceAction::Delete { .. } => soopy::SourceAction::Delete {
            expected: expected(rel)?,
            source,
        },
        soopy::SourceAction::Move { destination, .. } => soopy::SourceAction::Move {
            expected: expected(rel)?,
            source,
            destination: destination.clone(),
        },
        soopy::SourceAction::Replace {
            edits, expected, ..
        } => soopy::SourceAction::Replace {
            expected: expected.clone(),
            edits: edits
                .iter()
                .map(|edit| soopy::TextEdit {
                    range: soopy::ActionSpan {
                        source: source.clone(),
                        start: edit.range.start,
                        end: edit.range.end,
                    },
                    replacement: edit.replacement.clone(),
                    producer: edit.producer.clone(),
                })
                .collect(),
            source,
        },
    })
}
