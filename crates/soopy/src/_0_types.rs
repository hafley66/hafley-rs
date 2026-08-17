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

/// Serde bridge for immutable byte buffers. Span text serializes as bytes so
/// it preserves arbitrary source slices, including slices that split UTF-8
/// code points.
mod arc_bytes {
    use super::*;

    pub fn serialize<S: Serializer>(value: &Arc<[u8]>, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(value)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Arc<[u8]>, D::Error> {
        Ok(Arc::from(Vec::<u8>::deserialize(deserializer)?))
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

/// Identity of one canonical filesystem directory, independent of Git.
///
/// Construction: [`DirectoryRoot`](crate::DirectoryRoot) hashes its
/// canonical absolute path. The identity therefore has no repository,
/// revision, ref, index, or object-database coordinate.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DirectoryId(#[serde(with = "arc_str")] pub Arc<str>);

/// A UTF-8, `/`-separated file path relative to a [`DirectoryRoot`].
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RootPath(#[serde(with = "arc_str")] pub Arc<str>);

/// A filesystem-first coordinate for one file under a canonical directory.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FileRef {
    pub directory: DirectoryId,
    pub path: RootPath,
}

/// One file observed by a plain directory snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub file: FileRef,
    pub content: ContentId,
    pub size: u64,
}

/// A filesystem-first selection. Empty `patterns` selects every regular file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileQuery {
    pub patterns: Vec<Pattern>,
}

/// A stable result for one [`FileQuery`] execution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSnapshot {
    pub files: Vec<FileEntry>,
    pub directories: Vec<RootPath>,
}

/// One request for current bytes from a filesystem-first file coordinate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileReadRequest {
    pub file: FileRef,
    pub expected: Option<ContentId>,
}

/// Borrowed bytes provided by [`DirectoryRoot::read_each`](crate::DirectoryRoot::read_each).
/// The byte slice remains valid only for the visitor call.
#[derive(Clone, Copy, Debug)]
pub struct FileBytesRef<'a> {
    pub file: &'a FileRef,
    pub content: &'a ContentId,
    pub bytes: &'a [u8],
}

/// Filesystem selection and debounce policy for a directory watcher.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileWatchQuery {
    pub patterns: Vec<Pattern>,
    pub coalescing: WatchCoalescing,
}

/// A logical change between two plain-directory snapshots. Paths are relative
/// to the watched directory and contain no Git coordinate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectoryDelta {
    Added(PathBuf),
    Changed(PathBuf),
    Removed(PathBuf),
    RescanRequired,
}

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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Revision {
    Worktree,
    Named(#[serde(with = "arc_str")] Arc<str>),
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

impl ContentId {
    /// The one place the worktree hashing expression lives, so a caller
    /// outside this crate never re-derives it on its own blake3 dependency.
    pub fn blake3(bytes: &[u8]) -> Self {
        Self::Blake3(*blake3::hash(bytes).as_bytes())
    }
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEntry {
    pub source: SourceRef,
    pub content: ContentId,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

/// Borrowed source bytes provided by [`SourceTree::read_each`].
///
/// The view is valid only for the visitor call. Callers that need to retain
/// bytes beyond that call can copy them or use [`SourceTree::read_many`].
#[derive(Clone, Copy, Debug)]
pub struct SourceBytesRef<'a> {
    pub source: &'a SourceRef,
    pub content: &'a ContentId,
    pub bytes: &'a [u8],
}

/// A half-open byte range `[start, end)` within one revision-qualified source
/// file. The owning `SourceRef` is the stable coordinate that later maps to a
/// runtime `rev_file_id`; Soopy deliberately does not allocate dense row IDs.
///
/// Byte offsets need not be UTF-8 character boundaries. `span_text_many`
/// returns the exact bytes; `span_position_many` reports one-based lines and
/// zero-based byte columns so every valid byte boundary has a position.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SourceSpan {
    pub source: SourceRef,
    pub start: u64,
    pub end: u64,
}

/// One demand for the bytes in a `SourceSpan`. `expected` retains the same
/// replacement check as `ReadRequest` when callers carry a prior content ID.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpanTextRequest {
    pub span: SourceSpan,
    pub expected: Option<ContentId>,
}

/// The exact byte slice for one span request. It repeats the source content
/// identity as retrieval evidence, while `SourceSpan` remains the sole span
/// coordinate and stores no text itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpanText {
    pub span: SourceSpan,
    pub content: ContentId,
    #[serde(with = "arc_bytes")]
    pub bytes: Arc<[u8]>,
}

/// A source position with a one-based line and a zero-based byte column.
/// `byte_column` counts bytes from the preceding newline, rather than Unicode
/// scalar values or display columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BytePosition {
    pub line: u64,
    pub byte_column: u64,
}

/// One demand for line positions at the start and end of a `SourceSpan`.
///
/// `newline_index_byte_budget` is an explicit upper bound for the temporary
/// line-start index storage: `(newline count + 1) * size_of::<usize>()`.
/// A request above its budget fails before allocating that index.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpanPositionRequest {
    pub span: SourceSpan,
    pub expected: Option<ContentId>,
    pub newline_index_byte_budget: u64,
}

/// Start and exclusive-end positions for one `SourceSpan`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpanPosition {
    pub span: SourceSpan,
    pub content: ContentId,
    pub start: BytePosition,
    pub end: BytePosition,
}

/// One repository-local source selection. `SourceTree` supplies the repository;
/// the query selects one worktree or commit and a union of path globs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

/// Whether [`GitFileQuery`] includes filesystem paths not present in either
/// `HEAD` or the index. The default keeps the tracked surface bounded to Git's
/// tracked namespace; callers that need unknown filesystem paths opt in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UntrackedFilePolicy {
    #[default]
    Exclude,
    Include,
}

/// Selection for [`GitWorktreeRoot::tracked_state`](crate::GitWorktreeRoot::tracked_state).
/// `pathspecs` are passed to Git as Git pathspecs, as with [`GitFilesQuery`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitFileQuery {
    pub pathspecs: Vec<String>,
    pub untracked: UntrackedFilePolicy,
}

/// Git's normalized entry kind. `Gitlink` content is the nested commit OID;
/// it remains distinct from a blob by this field even though both are object
/// names in [`EntryIdentity::content`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum GitEntryKind {
    File,
    Symlink,
    Gitlink,
    Tree,
    Other,
}

/// Git's normalized tree/index mode, represented in its conventional octal
/// value (`0o100644`, `0o120000`, `0o160000`, ...).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GitEntryMode(pub u32);

/// The complete identity compared at each adjacent status transition. Content
/// IDs are insufficient: executable bit, symlink/file replacement, and
/// gitlink replacement are visible only through `kind` and `mode`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EntryIdentity {
    pub kind: GitEntryKind,
    pub mode: GitEntryMode,
    pub content: ContentId,
}

/// One index stage preserved for a path. A normal index entry has only stage
/// zero. Stages one through three make the row [`TrackedFileState::Unmerged`]
/// and are returned rather than being reduced to a synthetic stage-zero entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexStageEntry {
    pub stage: u8,
    pub identity: EntryIdentity,
}

/// The direct difference between two adjacent entries. The two transition
/// fields on an observation distinguish additions/deletions/modifications
/// without inferring them from an aggregate dirty bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryTransition {
    Unchanged,
    Added,
    Deleted,
    Modified,
    ModeChanged,
    TypeChanged,
    ModeAndContentChanged,
    TypeAndContentChanged,
    TypeAndModeChanged,
    TypeModeAndContentChanged,
}

/// Per-worktree `HEAD` availability. An unborn repository has no tree to
/// compare against; this remains visible independently of each path's missing
/// `head` entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackedHeadState {
    Present(ObjectId),
    Unborn,
}

/// The four comparable tracked states plus Git cases that have no valid
/// stage-zero/working-tree comparison. `Sparse` retains the index and HEAD
/// entries but does not claim an absent skipped worktree file is a deletion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackedFileState {
    Clean,
    Unstaged,
    Staged,
    StagedAndUnstaged,
    Unmerged,
    IntentToAdd,
    Sparse,
    Untracked,
    Unsupported(TrackedFileUnsupported),
}

/// A deterministic unsupported condition. Every unsupported representation
/// still returns its available HEAD/index/worktree identities and index stages.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackedFileUnsupported {
    GitlinkWorktreeUnavailable,
    WorktreeTypeUnavailable,
    WorktreeContentUnavailable,
}

/// One path's complete HEAD/index/worktree observation. `staged_change`
/// compares HEAD to index; `unstaged_change` compares index to worktree.
/// `None` marks a comparison intentionally unavailable for one of the typed
/// exceptional states, never a collapsed clean result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackedFileObservation {
    pub worktree: WorktreeId,
    pub path: RepoPath,
    pub state: TrackedFileState,
    pub head_state: TrackedHeadState,
    pub head: Option<EntryIdentity>,
    pub index: Option<EntryIdentity>,
    pub worktree_entry: Option<EntryIdentity>,
    pub index_stages: Vec<IndexStageEntry>,
    pub staged_change: Option<bool>,
    pub unstaged_change: Option<bool>,
    pub head_to_index: Option<EntryTransition>,
    pub index_to_worktree: Option<EntryTransition>,
}

/// Work performed by one tracked-state observation. `git_child_processes`
/// counts every Git child launched during that call, including a newly opened
/// persistent hash worker. `bytes_hashed` counts only worktree bytes sent to
/// the content hasher after a cache miss.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackedStateMetrics {
    pub git_child_processes: u64,
    pub hash_worker_launches: u64,
    pub byte_worker_launches: u64,
    pub bytes_hashed: u64,
    pub worktree_cache_hits: u64,
    pub worktree_cache_misses: u64,
}

/// The typed result of one tracked-state observation. The compatibility
/// [`GitWorktreeRoot::tracked_state`](crate::GitWorktreeRoot::tracked_state)
/// method returns only `observations`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackedStateResult {
    pub observations: Vec<TrackedFileObservation>,
    pub metrics: TrackedStateMetrics,
}

/// A directory coordinate derived from the selected source entries.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DirectoryEntry {
    pub repository: RepositoryId,
    pub revision: RevisionId,
    pub path: RepoPath,
}

/// A stable result for one `SourceQuery` execution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSnapshot {
    pub revision: RevisionId,
    pub files: Vec<SourceEntry>,
    pub directories: Vec<DirectoryEntry>,
}

/// A logical change between two snapshots of one worktree query.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

/// The attachment state of one worktree HEAD plus the commit it currently
/// resolves to. Symbolic branch names remain useful even when a ref query
/// filters that branch out; `target` carries the resolved commit independently.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct HeadObservation {
    pub state: Head,
    pub target: Option<ObjectId>,
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
    /// The symbolic, detached, or unborn attachment state of this checkout.
    pub head: Head,
    /// The commit `HEAD` resolves to, independent of selected named refs.
    pub head_target: Option<ObjectId>,
    pub refs: Vec<RefObservation>,
}

/// A ref-level transition between two snapshots of one repository: a ref
/// arriving, leaving, or changing its target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefDelta {
    HeadChanged {
        before: HeadObservation,
        after: HeadObservation,
    },
    Added(RefObservation),
    Removed(RefObservation),
    Changed {
        before: RefObservation,
        after: RefObservation,
    },
}

/// One validated debounce policy for a repository watcher. Durations are plain
/// milliseconds so typed requests remain portable across runtime boundaries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchCoalescing {
    pub quiet_ms: u32,
    pub max_ms: u32,
}

impl Default for WatchCoalescing {
    fn default() -> Self {
        Self {
            quiet_ms: 120,
            max_ms: 600,
        }
    }
}

/// The selected repository surfaces for one watcher. A source surface is
/// limited to `Revision::Worktree`; immutable commits have no watchable state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchQuery {
    pub source: Option<SourceQuery>,
    pub refs: Option<RefQuery>,
    pub index: bool,
    pub linked_worktrees: bool,
    pub coalescing: WatchCoalescing,
}

/// Identity of the logical staged-index entry set. It is the BLAKE3 digest of
/// `git ls-files --stage -z`, preserving stage, mode, blob, and path data.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct IndexId(pub [u8; 32]);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexSnapshot {
    pub repository: RepositoryId,
    /// The checkout whose index was observed. Index identity deliberately does
    /// not carry the mutable worktree's HEAD or dirty flags: unstaged writes
    /// do not change the staged index entry set.
    pub worktree: WorktreeId,
    pub index: IndexId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexDelta {
    Changed {
        before: IndexSnapshot,
        after: IndexSnapshot,
    },
}

/// One live linked checkout. `head` is derived from `git worktree list
/// --porcelain`; a removed checkout remains observable through its prior row.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct WorktreeObservation {
    pub repository: RepositoryId,
    pub worktree: WorktreeId,
    pub root: PathBuf,
    /// The commit recorded by `git worktree list --porcelain`, separate from
    /// the branch attachment state so an advancing symbolic branch is visible.
    pub commit: Option<ObjectId>,
    pub head: Head,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeSnapshot {
    pub repository: RepositoryId,
    pub worktrees: Vec<WorktreeObservation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorktreeDelta {
    Added(WorktreeObservation),
    Removed(WorktreeObservation),
    Changed {
        before: WorktreeObservation,
        after: WorktreeObservation,
    },
}

/// Stable state for the selected watch surfaces. It is available after `open`
/// and every successful rescan, and is ordered by source paths, ref names, and
/// worktree IDs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositorySnapshot {
    pub repository: RepositoryId,
    pub source: Option<SourceSnapshot>,
    pub refs: Option<RefSnapshot>,
    pub index: Option<IndexSnapshot>,
    pub worktrees: Option<WorktreeSnapshot>,
}

/// A repository-scoped event envelope. Soopy owns the snapshots and deltas;
/// the DL6 runtime owns clocks, rev_advanced rows, and retractions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepositoryDelta {
    Ref(RefDelta),
    Source(SourceDelta),
    Index(IndexDelta),
    Worktree(WorktreeDelta),
    RescanRequired,
}

/// Outcome of resolving one revision to a commit object.
///
/// `Present` is a commit that exists in the object database and is not a
/// shallow boundary. `Absent` is a revision that does not resolve to any
/// commit. `ShallowBoundary` is a commit present locally but whose parents
/// were cut by a shallow clone or deepen, so it is the tip of the locally
/// available history. `CorruptObject` is an object that exists on disk but
/// cannot be read or parsed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RevisionResolution {
    Present(ObjectId),
    Absent,
    ShallowBoundary(ObjectId),
    CorruptObject,
}

/// The direct parents of one commit, in the order the commit object records
/// them. The first entry is the first parent (the branch advanced), the rest
/// are merged-in parents.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitParents {
    pub commit: ObjectId,
    pub parents: Vec<ObjectId>,
}

/// The answer to one ancestry question: is `ancestor` reachable from
/// `descendant`?
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ancestry {
    pub ancestor: ObjectId,
    pub descendant: ObjectId,
    pub is_ancestor: bool,
}

/// The merge bases of two commits. `bases` is empty when the commits share no
/// history; otherwise it lists every best common ancestor, sorted by OID.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeBase {
    pub left: ObjectId,
    pub right: ObjectId,
    pub bases: Vec<ObjectId>,
}

/// Symmetric commit counts between two commits: `ahead` counts commits
/// reachable from `left` but not `right`, `behind` the reverse.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AheadBehind {
    pub left: ObjectId,
    pub right: ObjectId,
    pub ahead: u64,
    pub behind: u64,
}

/// A deterministic walk of the commits reachable from one peeled start. The
/// start is the commit the revision peels to (a lightweight or annotated tag
/// peels before the walk), and `commits` lists every reachable commit in
/// stable topological order, newest first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitWalk {
    pub start: ObjectId,
    pub commits: Vec<ObjectId>,
}

/// A batched revision-graph request: several resolutions, parent lookups,
/// ancestry questions, merge-base pairs, ahead/behind pairs, and walks in one
/// call. Every input list is answered in order; the result preserves that
/// order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionGraphQuery {
    pub repository: RepositoryId,
    pub resolve: Vec<Revision>,
    pub parents: Vec<ObjectId>,
    pub ancestry: Vec<(ObjectId, ObjectId)>,
    pub merge_bases: Vec<(ObjectId, ObjectId)>,
    pub ahead_behind: Vec<(ObjectId, ObjectId)>,
    pub walks: Vec<Revision>,
}

/// The batched result of one `RevisionGraphQuery`, with every vector parallel
/// to its request. `parents`, `ancestry`, `merge_bases`, and `ahead_behind`
/// list entries correspond one-to-one with their query lists; `resolutions`
/// and `walks` mirror `resolve` and `walks`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionGraphResult {
    pub repository: RepositoryId,
    pub resolutions: Vec<RevisionResolution>,
    pub parents: Vec<CommitParents>,
    pub ancestry: Vec<Ancestry>,
    pub merge_bases: Vec<MergeBase>,
    pub ahead_behind: Vec<AheadBehind>,
    pub walks: Vec<CommitWalk>,
}

/// The set of network mutations an acquisition request is permitted to
/// perform. The default rejects everything, so read-only callers and callers
/// that forget to opt in can never fetch, unshallow, or update refs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcquisitionPolicy {
    /// Permit fetching branch refs from a remote.
    pub allow_fetch: bool,
    /// Permit fetching tag refs from a remote.
    pub allow_tag_fetch: bool,
    /// Permit deepening or fully unshallowing a shallow clone.
    pub allow_unshallow: bool,
}

/// One permitted acquisition operation, carrying the remote and target it
/// describes. Every operation is gated by the matching policy flag before any
/// Git process is spawned.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AcquisitionOperation {
    /// Fetch one branch ref from a remote.
    FetchRef {
        #[serde(with = "arc_str")]
        remote: Arc<str>,
        #[serde(with = "arc_str")]
        name: Arc<str>,
    },
    /// Fetch one tag ref from a remote.
    FetchTag {
        #[serde(with = "arc_str")]
        remote: Arc<str>,
        #[serde(with = "arc_str")]
        name: Arc<str>,
    },
    /// Deepen a shallow clone by `depth` more commits.
    Deepen {
        #[serde(with = "arc_str")]
        remote: Arc<str>,
        depth: u32,
    },
    /// Fully unshallow the clone.
    Unshallow {
        #[serde(with = "arc_str")]
        remote: Arc<str>,
    },
}

/// A batched acquisition request: the repository plus an ordered list of
/// operations whose receipts are returned in order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcquisitionRequest {
    pub repository: RepositoryId,
    pub operations: Vec<AcquisitionOperation>,
}

/// The typed receipt for one acquisition operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AcquisitionReceipt {
    /// The ref already resolved locally; no fetch was needed.
    AlreadyPresent {
        direct: ObjectId,
        peeled: Option<ObjectId>,
    },
    /// A branch ref was fetched and now resolves to `target`.
    FetchedRef {
        #[serde(with = "arc_str")]
        name: Arc<str>,
        target: ObjectId,
    },
    /// A tag ref was fetched and now resolves to `target`.
    FetchedTag {
        #[serde(with = "arc_str")]
        name: Arc<str>,
        direct: ObjectId,
        peeled: Option<ObjectId>,
    },
    /// The clone was deepened by `depth` commits.
    Deepened { depth: u32 },
    /// The clone was fully unshallowed.
    Unshallowed,
    /// The repository already had complete history.
    AlreadyComplete,
    /// The operation was permitted but could not complete (e.g. the remote is
    /// absent or unreachable).
    Unavailable {
        #[serde(with = "arc_str")]
        reason: Arc<str>,
    },
    /// The policy rejected the operation before any Git process ran.
    RejectedByPolicy,
}

/// One acquisition result paired with the operation that produced it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcquisitionOutcome {
    pub operation: AcquisitionOperation,
    pub receipt: AcquisitionReceipt,
}
