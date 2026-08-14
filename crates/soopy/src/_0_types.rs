use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use crate::_1_pattern::Pattern;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepositoryId(pub Arc<str>);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(pub Arc<str>);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepoPath(pub Arc<str>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repository {
    pub root: PathBuf,
    pub identity: RepositoryId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Revision {
    Worktree,
    Named(Arc<str>),
    Commit(ObjectId),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RevisionId {
    Worktree { head: Option<ObjectId>, dirty: bool },
    Commit(ObjectId),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContentId {
    GitBlob(ObjectId),
    Blake3([u8; 32]),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceRef {
    pub repository: RepositoryId,
    pub revision: RevisionId,
    pub path: RepoPath,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceEntry {
    pub source: SourceRef,
    pub content: ContentId,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadRequest {
    pub source: SourceRef,
    pub expected: Option<ContentId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceBytes {
    pub source: SourceRef,
    pub content: ContentId,
    pub bytes: Arc<[u8]>,
}

/// One repository-local source selection. `SourceTree` supplies the repository;
/// the query selects one worktree or commit and a union of path globs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceQuery {
    pub revision: Revision,
    pub patterns: Vec<Pattern>,
}

/// Git's tracked-file query contract. `pathspecs` travel unchanged to
/// `git ls-files`; they are Git pathspecs rather than `globset` patterns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitFilesQuery {
    pub revision: Revision,
    pub pathspecs: Vec<String>,
}

/// A directory coordinate derived from the selected source entries.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DirectoryEntry {
    pub repository: RepositoryId,
    pub revision: RevisionId,
    pub path: RepoPath,
}

/// A stable result for one `SourceQuery` execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSnapshot {
    pub revision: RevisionId,
    pub files: Vec<SourceEntry>,
    pub directories: Vec<DirectoryEntry>,
}

/// A logical change between two snapshots of one worktree query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceDelta {
    Added(SourceEntry),
    Changed { before: SourceEntry, after: SourceEntry },
    Removed(SourceRef),
    RevisionChanged { before: RevisionId, after: RevisionId },
    RescanRequired,
}

impl fmt::Display for ContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GitBlob(oid) => write!(f, "git:{}", oid.0),
            Self::Blake3(bytes) => write!(f, "blake3:{}", blake3::Hash::from_bytes(*bytes)),
        }
    }
}
