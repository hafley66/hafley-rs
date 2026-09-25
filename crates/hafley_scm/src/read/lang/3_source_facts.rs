//! Canonical source-query facts shared by Soopy, Extract, and DL7.
//!
//! The envelope stores source placement and content once. Every range is a
//! content span. DL7 derives located occurrences by pairing those spans with
//! the envelope's source value.

use super::{query_tree_sitter_spans, SourceQuery, SourceQueryError};
use crate::read::shape::content_id_of;
use serde::Serialize;
use serde_json::Value;

pub const SOURCE_FACT_PROTOCOL: u32 = 1;

/// Transport form of Soopy's source sum. Every identity component remains a
/// separate field so a relational loader never has to decode display text.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourcePlace {
    Directory {
        directory: String,
        path: String,
    },
    Git {
        repository: String,
        revision: SourceRevisionFact,
        path: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceRevisionFact {
    Worktree {
        worktree: String,
        head: Option<String>,
        dirty: bool,
    },
    Commit {
        object: String,
    },
}

impl From<&soopy::ActionSource> for SourcePlace {
    fn from(source: &soopy::ActionSource) -> Self {
        match source {
            soopy::ActionSource::Directory { file } => Self::Directory {
                directory: file.directory.0.to_string(),
                path: file.path.0.to_string(),
            },
            soopy::ActionSource::Git { source } => Self::Git {
                repository: source.repository.0.to_string(),
                revision: match &source.revision {
                    soopy::RevisionId::Worktree {
                        worktree,
                        head,
                        dirty,
                    } => SourceRevisionFact::Worktree {
                        worktree: worktree.0.to_string(),
                        head: head.as_ref().map(|object| object.0.to_string()),
                        dirty: *dirty,
                    },
                    soopy::RevisionId::Commit(object) => SourceRevisionFact::Commit {
                        object: object.0.to_string(),
                    },
                },
                path: source.path.0.to_string(),
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GitBlobFact {
    pub repository: String,
    pub object: String,
}

/// One half-open range in the envelope's canonical content. The envelope owns
/// the content identity once and the DL7 loader expands each range into a
/// `content_span(Content, Start, End)` value.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub struct ByteRange {
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SourceParseFact {
    pub grammar: String,
    pub engine: String,
    pub version: String,
    pub configuration: Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SourceQueryFact {
    pub engine: String,
    pub specification: Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SourceCaptureFact {
    pub position: u32,
    pub label: String,
    pub range: ByteRange,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SourceReplacementFact {
    pub replacement: String,
    pub producer: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SourceMatchFact {
    pub position: u32,
    pub branch: String,
    pub pattern: u32,
    pub range: ByteRange,
    pub captures: Vec<SourceCaptureFact>,
    pub replacement: Option<SourceReplacementFact>,
}

/// One normalized query answer. Repeated content, source, parse, and query
/// values occur once; match-local rows retain only their ordered differences.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SourceQueryFacts {
    pub protocol: u32,
    pub source: SourcePlace,
    pub content: String,
    pub byte_length: u64,
    pub git_blobs: Vec<GitBlobFact>,
    pub parse: SourceParseFact,
    pub query: SourceQueryFact,
    pub matches: Vec<SourceMatchFact>,
}

/// Normalize one query over bytes read by Soopy. A worktree or directory
/// BLAKE3 observation is checked against the bytes. A Git blob observation is
/// retained as a capability of the canonical BLAKE3 content value.
pub fn query_source_facts(
    source: &soopy::ActionSource,
    observed_content: &soopy::ContentId,
    content: &[u8],
    query: &SourceQuery,
) -> Result<SourceQueryFacts, SourceQueryError> {
    let canonical = content_id_of(content);
    if matches!(observed_content, soopy::ContentId::Blake3(_)) && observed_content != &canonical {
        return Err(SourceQueryError::Projection(format!(
            "source bytes hash to {canonical}, observed {observed_content}"
        )));
    }
    let content_name = canonical.to_string();
    let source_place = SourcePlace::from(source);
    let git_blobs = git_blob_facts(source, observed_content)?;
    let (engine, grammar, matches) = normalized_matches(content, query)?;
    let specification = query_specification(query)?;
    Ok(SourceQueryFacts {
        protocol: SOURCE_FACT_PROTOCOL,
        source: source_place,
        content: content_name.clone(),
        byte_length: content.len() as u64,
        git_blobs,
        parse: SourceParseFact {
            grammar,
            engine: "tree-sitter".to_string(),
            version: crate::read::RYI_VERSION.to_string(),
            configuration: Value::Object(Default::default()),
        },
        query: SourceQueryFact {
            engine,
            specification,
        },
        matches,
    })
}

fn git_blob_facts(
    source: &soopy::ActionSource,
    observed: &soopy::ContentId,
) -> Result<Vec<GitBlobFact>, SourceQueryError> {
    match (source, observed) {
        (soopy::ActionSource::Git { source }, soopy::ContentId::GitBlob(object)) => {
            Ok(vec![GitBlobFact {
                repository: source.repository.0.to_string(),
                object: object.0.to_string(),
            }])
        }
        (soopy::ActionSource::Directory { .. }, soopy::ContentId::GitBlob(_)) => {
            Err(SourceQueryError::Projection(
                "a directory source cannot carry a repository-scoped Git blob".to_string(),
            ))
        }
        (_, soopy::ContentId::Blake3(_)) => Ok(Vec::new()),
    }
}

fn normalized_matches(
    content: &[u8],
    query: &SourceQuery,
) -> Result<(String, String, Vec<SourceMatchFact>), SourceQueryError> {
    match query {
        SourceQuery::TreeSitter(request) => {
            let found =
                query_tree_sitter_spans(content, request).map_err(SourceQueryError::TreeSitter)?;
            let matches = found
                .into_iter()
                .enumerate()
                .map(|(position, found)| SourceMatchFact {
                    position: position as u32,
                    branch: "query".to_string(),
                    pattern: found.pattern,
                    range: byte_range(found.start, found.end),
                    captures: found
                        .captures
                        .into_iter()
                        .enumerate()
                        .map(|(position, capture)| SourceCaptureFact {
                            position: position as u32,
                            label: capture.label,
                            range: byte_range(capture.start, capture.end),
                        })
                        .collect(),
                    replacement: None,
                })
                .collect();
            Ok(("tree_sitter".to_string(), request.language.clone(), matches))
        }
    }
}

fn byte_range(start: u32, end: u32) -> ByteRange {
    ByteRange { start, end }
}

fn query_specification(query: &SourceQuery) -> Result<Value, SourceQueryError> {
    let result = match query {
        SourceQuery::TreeSitter(value) => serde_json::to_value(value),
    };
    result.map_err(|error| SourceQueryError::Projection(error.to_string()))
}
