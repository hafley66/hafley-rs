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

/// Serde bridge for `Option<Arc<str>>`, mirroring `arc_str` for optional string
/// fields. It serializes `None` as a JSON null and `Some` as the plain string.
mod opt_arc_str {
    use super::*;

    pub fn serialize<S: Serializer>(
        value: &Option<Arc<str>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(value) => serializer.serialize_some(value.as_ref()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Arc<str>>, D::Error> {
        Ok(Option::<String>::deserialize(deserializer)?.map(Arc::from))
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

/// The kind of Git object a ref points at or a tag peels to.
///
/// Serialization is the lowercase Git object-type spelling (`blob`, `tree`,
/// `commit`, `tag`), which is the value `git for-each-ref` and `cat-file`
/// report, never a Rust variant name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ObjectKind {
    Blob,
    Tree,
    Commit,
    Tag,
}

/// The tagger identity recorded on an annotated tag object.
///
/// `when` is the whole-second Unix timestamp from `%(taggerdate:unix)`, chosen
/// over a localized date string so serialization is deterministic.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Tagger {
    #[serde(with = "arc_str")]
    pub name: Arc<str>,
    #[serde(with = "arc_str")]
    pub email: Arc<str>,
    pub when: i64,
}

/// Metadata carried by an annotated tag object, present only when a ref's
/// direct object is itself a `tag`. The peeled target object is recorded on
/// the enclosing observation; this carries the target's kind and the tagger
/// and message authored into the tag object.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TagMetadata {
    /// Kind of the object the tag peels to (its target), normally `Commit`.
    pub target_kind: ObjectKind,
    pub tagger: Tagger,
    #[serde(with = "arc_str")]
    pub message: Arc<str>,
}

/// One observation of a ref: its full name, symbolic target when present, and
/// the object identity it resolves to.
///
/// `direct` is the unpeeled object the ref names; `peeled` is `Some` only when
/// `direct` is a tag object and holds the peeled-through target. `kind` is the
/// kind of `direct`. `tag` carries annotated-tag metadata when `kind` is `Tag`.
///
/// Equality/ordering: structural over `(repository, name, ...)`.
///
/// Serialization: structural, with object names as hex strings.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RefObservation {
    pub repository: RepositoryId,
    /// Full ref name, e.g. `refs/heads/main`.
    #[serde(with = "arc_str")]
    pub name: Arc<str>,
    /// Symbolic target when the ref is itself a symref (e.g. `refs/remotes/origin/HEAD`).
    #[serde(with = "opt_arc_str")]
    pub symbolic: Option<Arc<str>>,
    pub direct: ObjectId,
    pub peeled: Option<ObjectId>,
    pub kind: ObjectKind,
    pub tag: Option<TagMetadata>,
}

/// The per-worktree `HEAD` state.
///
/// `Symbolic` names another ref (normally a branch); the resolved commit is
/// already carried by that branch's own observation. `Detached` points
/// directly at a commit, which is not present in the named-ref set. `Unborn`
/// names a branch that has no commit yet.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Head {
    Symbolic {
        #[serde(with = "arc_str")]
        target: Arc<str>,
    },
    Detached(ObjectId),
    Unborn {
        #[serde(with = "arc_str")]
        target: Arc<str>,
    },
}

/// A repository-scoped ref selection.
///
/// `namespace` is a ref prefix such as `refs/heads`, `refs/tags`, or
/// `refs/remotes`; empty enumerates every ref. `name` selects one exact full
/// ref name and takes precedence over `pattern`, a glob over full ref names.
/// Both are optional.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RefQuery {
    pub repository: RepositoryId,
    #[serde(with = "arc_str")]
    pub namespace: Arc<str>,
    #[serde(with = "opt_arc_str")]
    pub name: Option<Arc<str>>,
    #[serde(with = "opt_arc_str")]
    pub pattern: Option<Arc<str>>,
}

/// A deterministic collection of ref observations for one repository and one
/// worktree's `HEAD`.
///
/// `refs` is sorted by full ref name, and every field serializes structurally,
/// so two equal snapshots serialize to one byte string.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefSnapshot {
    pub repository: RepositoryId,
    pub head: Head,
    pub refs: Vec<RefObservation>,
}

/// A ref-level transition between two snapshots of one repository: a ref
/// arriving, leaving, or changing its target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefDelta {
    Added(RefObservation),
    Removed(RefObservation),
    Changed {
        before: RefObservation,
        after: RefObservation,
    },
}

/// A repository-scoped event envelope for the watch surface. It carries source
/// and ref deltas plus a rescan condition, with no DL6 clocks or retractions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepositoryDelta {
    Ref(RefDelta),
    Source(SourceDelta),
    RescanRequired,
}
