use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::_1_pattern::Pattern;

/// Serde bridge for `Arc<str>`, which has no derived `Deserialize` because it
/// is unsized. Coordinates serialize as their plain string key and deserialize
/// back into an `Arc<str>`, independent of any `Display` spelling.
mod arc_str {
    use super::*;

    pub fn serialize<S: Serializer>(value: &Arc<str>, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(value.as_ref())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Arc<str>, D::Error> {
        Ok(Arc::from(String::deserialize(deserializer)?))
    }
}

/// Identity of one shared Git object database / logical repository.
///
/// Construction: `crate::_2_repository::open` hashes the canonicalized
/// `git rev-parse --git-common-dir` path. Linked worktrees share the common
/// directory, so they share one `RepositoryId`.
///
/// Equality/ordering: structural over the hashed key.
///
/// Serialization: the hex key string; never a display string.
///
/// Lifetime: stable while the repository's common directory path is stable.
///
/// Uniqueness: one value per common Git directory.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RepositoryId(#[serde(with = "arc_str")] pub Arc<str>);

/// Identity of one checkout root within a repository.
///
/// Construction: `crate::_2_repository::open` hashes the canonicalized
/// `git rev-parse --absolute-git-dir` path. The main worktree hashes the
/// common `.git` directory; each linked worktree hashes its own
/// `worktrees/<name>` directory, so distinct worktrees are distinct and
/// stable across reopen.
///
/// Equality/ordering: structural over the hashed key.
///
/// Serialization: the hex key string.
///
/// Lifetime: stable while the checkout's Git directory path is stable.
///
/// Uniqueness: one value per checkout root; a linked worktree never shares
/// its `WorktreeId` with the main worktree or a sibling.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct WorktreeId(#[serde(with = "arc_str")] pub Arc<str>);

/// Identity of one Git object: a blob, tree, commit, or tag OID.
///
/// Construction: from a `git rev-parse`/`git ls-tree`/`git hash-object`
/// hexadecimal object name.
///
/// Equality/ordering: structural over the hex string.
///
/// Serialization: the hex string.
///
/// Lifetime: the object database lifetime.
///
/// Uniqueness: one value per object name within its repository.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ObjectId(#[serde(with = "arc_str")] pub Arc<str>);

/// A repository-qualified full ref name, e.g. `refs/heads/main`.
///
/// Construction: `RefId::new(repository, name)`, or any caller pairing a
/// `RepositoryId` with a full ref name. Ref enumeration is out of scope; this
/// is the coordinate, not a traversal.
///
/// Equality/ordering: `(repository, name)` lexicographic.
///
/// Serialization: `{ repository, name }`.
///
/// Lifetime: stable while the repository and ref name both exist.
///
/// Uniqueness: one value per `(repository, full ref name)` pair.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RefId {
    pub repository: RepositoryId,
    #[serde(with = "arc_str")]
    pub name: Arc<str>,
}

impl RefId {
    pub fn new(repository: RepositoryId, name: Arc<str>) -> Self {
        Self { repository, name }
    }
}

/// A repository-relative, `/`-separated, UTF-8 source path.
///
/// Construction: by the worktree walker, `git ls-tree`, or `git ls-files`,
/// each stripping the repository root and normalizing separators.
///
/// Equality/ordering: structural over the path string.
///
/// Serialization: the path string.
///
/// Lifetime: stable while the path exists in its repository.
///
/// Uniqueness: one value per repository-relative path spelling. Non-UTF-8 and
/// newline-bearing paths are rejected at construction.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RepoPath(#[serde(with = "arc_str")] pub Arc<str>);

/// An opened repository: one checkout root plus its two identities.
///
/// `identity` is the shared `RepositoryId`; `worktree` is the per-checkout
/// `WorktreeId`. This type is an open handle, not a serialized coordinate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repository {
    pub root: PathBuf,
    pub identity: RepositoryId,
    pub worktree: WorktreeId,
}

/// A revision selection supplied by a caller, before resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Revision {
    Worktree,
    Named(Arc<str>),
    Commit(ObjectId),
}

/// A resolved revision coordinate.
///
/// `Worktree` carries the checkout's `WorktreeId` plus the observed `HEAD` and
/// dirty flag, so a worktree coordinate cannot alias a sibling checkout.
/// `Commit` is repository-scoped: it names an immutable object reachable from
/// any linked worktree that shares the object database.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RevisionId {
    Worktree {
        worktree: WorktreeId,
        head: Option<ObjectId>,
        dirty: bool,
    },
    Commit(ObjectId),
}

/// Identity of source bytes: a Git blob OID or a BLAKE3 digest.
///
/// Worktree enumeration emits `Blake3`; tracked-file and committed enumeration
/// emit `GitBlob`. Serialization is structural, never the `Display` spelling.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ContentId {
    GitBlob(ObjectId),
    Blake3([u8; 32]),
}

/// A stable coordinate for one file at one revision.
///
/// The revision field carries worktree identity for worktree coordinates and
/// is repository-scoped for commit coordinates. `read_many` rejects a
/// worktree `SourceRef` opened through a different checkout.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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
    Changed {
        before: SourceEntry,
        after: SourceEntry,
    },
    Removed(SourceRef),
    RevisionChanged {
        before: RevisionId,
        after: RevisionId,
    },
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
