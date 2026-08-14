# soopy

`soopy` supplies one Rust interface for filesystem worktrees and immutable
Git revisions. It enumerates repository-relative paths and folders, assigns
content identities, reads blobs in batches, and reports debounced logical
filesystem and ref deltas. The crate contains no Sprefa types or runtime
assumptions.

## Data model

```text
Repository + Revision + Pattern[]
    -> SourceQuery -> SourceSnapshot
    -> files: SourceEntry { SourceRef, ContentId, size }
    -> directories: DirectoryEntry
    -> ReadRequest[]
    -> SourceBytes[]

SourceSnapshot + filesystem / Git ref events
    -> SourceDelta { Added, Changed, Removed, RevisionChanged, RescanRequired }
```

Worktree entries use BLAKE3 content IDs. Committed entries retain Git blob OIDs.
Committed reads share one persistent `git cat-file --batch` process per
`SourceTree` instance. Worktree snapshots retain `(mtime seconds, size,
BLAKE3)` metadata and rehash entries inside the prior walk's timestamp second.
The worktree walker honors ignore rules, excludes `.git`, and prunes nested
repositories. Git revisions use `git ls-tree`; Git CLI remains the object
database backend.

## Source coordinate identities

Soopy owns the stable source coordinates and serializable request/result
types. Dense relational IDs (`FileId`, `RevFileId`, `BlobId`, `FileSpanId`)
are out of scope and belong to `source-identity-mapping`.

| Type | Construction | Uniqueness | Lifetime |
|---|---|---|---|
| `RepositoryId` | hash of canonicalized `--git-common-dir` | one per common Git directory | repository lifetime |
| `WorktreeId` | hash of canonicalized `--absolute-git-dir` | one per checkout root | checkout lifetime |
| `RevisionId` | `Worktree` carries `WorktreeId` + `HEAD` + dirty; `Commit` carries an OID | commit: immutable OID; worktree: one observation | commit: object DB; worktree: one snapshot |
| `RefId` | `(RepositoryId, full ref name)` | one per `(repo, name)` | while ref exists |
| `ObjectId` | a Git object name | one per object name | object DB lifetime |
| `RepoPath` | repository-relative, `/`-separated UTF-8 | one per path spelling | while path exists |
| `ContentId` | `GitBlob` OID or `Blake3` digest | one per byte identity | content lifetime |
| `SourceRef` | `(RepositoryId, RevisionId, RepoPath)` | one file at one revision | placement lifetime |

`RevisionId::Worktree` carries the checkout's `WorktreeId`, so a worktree
coordinate cannot alias a sibling linked checkout. `RevisionId::Commit` is
repository-scoped and readable from any linked worktree that shares the object
database. Every public coordinate derives `Serialize`/`Deserialize` over its
plain key (string or byte array), never a `Display` string, and round-trips
through `serde_json`.

## Implementation boundary

Soopy's normal library and CLI paths are intended to remain Rust-native except
for Git operations. Git retains ownership of repository discovery, revision
resolution, the index, refs, pathspec evaluation, worktree operations, and the
object database. Soopy may invoke the `git` executable for those mechanics.

| Surface | Current implementation | Intended implementation |
|---|---|---|
| Filesystem traversal | `ignore` | `ignore` |
| Path matching | `globset` | `globset` |
| Filesystem events | `notify` | `notify` |
| Worktree hashing | `blake3` | `blake3` |
| Git trees, index, refs, and objects | `git` subprocesses | explicit Git backend using `git` |
| Text search | `rg` subprocess | `grep-searcher`, `grep-matcher`, `grep-regex` |
| Fuzzy ranking | `fzf` subprocess | high-level `nucleo` |
| Selection command surface | inherited `fzf` process | `clap` arguments over typed `nucleo` results |

The Rust search path should consume `SourceQuery` and return source coordinates
and spans. The selection path should consume stable result identities and
return ranked or selected identities. The existing `clap` surface owns query,
limit, output, and selection arguments.

```text
SourceQuery
    -> SourceSnapshot
    -> SearchQuery
    -> SearchMatch { source, content, span }
    -> SelectionQuery
    -> Selection { match identity, rank }
```

## Watch coverage

The current `SourceWatcher` owns one repository, one `Revision::Worktree`
query, one prior snapshot, and one worktree cache. It watches the source root
and selected Git ref paths, coalesces native events for 120–600 ms, computes a
fresh snapshot, and emits `Added`, `Changed`, `Removed`, `RevisionChanged`, or
`RescanRequired`.

The repository watch surface still needs typed events for:

- Git worktree creation and removal;
- checkout attachment or detachment;
- HEAD and named-ref movement;
- index changes;
- repository-set changes;
- simultaneous watched worktrees and queries;
- search-result deltas derived from source deltas.

These events should preserve separate repository, worktree, revision, path,
and content identities. Filesystem event paths are inputs to that model rather
than the public event vocabulary.

## Planned library surfaces

```rust
pub trait SourceBackend {
    fn snapshot(&mut self, query: &SourceQuery) -> anyhow::Result<SourceSnapshot>;
    fn read_many(&mut self, requests: &[ReadRequest])
        -> anyhow::Result<Vec<SourceBytes>>;
}

pub trait SourceWatch {
    fn recv(&mut self) -> anyhow::Result<Vec<SourceDelta>>;
}

pub trait RepositoryWatch {
    fn recv(&mut self) -> anyhow::Result<Vec<RepositoryDelta>>;
}

pub trait SourceSearch {
    fn search(&mut self, query: &SearchQuery)
        -> anyhow::Result<Vec<SearchMatch>>;
}

pub trait SourceSelect<T> {
    fn rank(&mut self, query: &SelectionQuery, values: &[T])
        -> anyhow::Result<Vec<Selection>>;
}
```

## Correctness invariants

These invariants are enforced by the implementation and pinned by `tests/1_correctness.rs`:

1. `repo_files` pathspecs are relative to the selected repository root;
   unscoped `files` pathspecs are relative to the caller's working directory.
2. Every worktree read validates a repository-relative path before filesystem
   access.
3. Every committed read validates the requested `commit:path` against its
   expected blob identity.
4. Every content identity returned by enumeration round-trips through
   `SourceTree::read_many`.
5. Worktree caches release entries absent from the latest completed walk.
6. Tracked symlink behavior agrees across worktree and commit snapshots
   (symlinks are dropped in both).
7. Repository identity and worktree identity are separate and tested across
   linked worktrees: linked worktrees share `RepositoryId` but have distinct
   `WorktreeId`, and reopening a checkout reproduces both.
8. Failed Git commands cannot produce clean or complete source coordinates.
9. Non-UTF-8 and newline-bearing repository paths are rejected explicitly,
   because `RepoPath` is a UTF-8 `Arc<str>` and the Git batch protocols are
   line-oriented. A repository containing either cannot be enumerated by
   `git_files` or `read_many`, which errors rather than collapsing or corrupting
   the coordinate.
10. A worktree `SourceRef` opened through one checkout cannot be read through a
    different checkout: `read_many` rejects a `RevisionId::Worktree` whose
    `WorktreeId` differs from the open `SourceTree`'s checkout. Commit reads
    stay repository-scoped and succeed from any linked worktree sharing the
    object database.

## CLI

The binary is a thin command adapter over the source API. `--repo` accepts a
repository root or any path inside it. `WORK` selects mutable worktree bytes;
any other revision is resolved by Git and selects immutable committed bytes.

| Command | Input | Output | Backend |
|---|---|---|---|
| `resolve REV` | `WORK`, ref, tag, or commit expression | resolved `RevisionId` | `SourceTree::resolve_revision` |
| `files` | revision plus repeated globs | path, content identity, size | `SourceTree::snapshot` |
| `read` | revision plus repeated globs | verified bytes for every match | `snapshot` then `read_many` |
| `watch` | repeated worktree globs | coalesced source deltas | `SourceTree::watch` |
| `query PATTERN` | text pattern plus repeated globs | matching text records | CLI adapter over `rg` |

`files`, `read`, and `watch` use typed Soopy library operations. `query` is a
CLI-only process adapter. `--fzf` sends its output to the installed `fzf`
binary. Revision-graph and acquisition operations currently have a Rust API
and no CLI subcommands.

Build and inspect the complete command reference:

```sh
cargo run -p soopy -- --help
cargo run -p soopy -- files --help
cargo run -p soopy -- read --help
cargo run -p soopy -- watch --help
cargo run -p soopy -- query --help
```

Resolve the current worktree identity:

```sh
soopy --repo . resolve WORK
```

Resolve a branch, tag, or commit:

```sh
soopy --repo . resolve HEAD~1
```

Enumerate the worktree with several patterns in one traversal:

```sh
soopy --repo . files \
  --revision WORK \
  --glob '**/*.rs' \
  --glob '**/*.ts'
```

Enumerate an immutable revision:

```sh
soopy --repo . files --revision HEAD --glob '**/*.rs'
```

Read matching committed blobs through one `git cat-file --batch` process:

```sh
soopy --repo . read --revision HEAD --glob 'src/**/*.rs'
```

Watch a worktree and print repository-relative changed paths:

```sh
soopy --repo . watch
```

Use `--format jsonl` with `files`, `read`, or `watch` for structured records.
`files` defaults to tab-separated `path`, `content ID`, and `size`; `read`
defaults to a `path<TAB>byte-count` header followed by raw file bytes.

Text search and selection stay at the command edge:

```sh
soopy --repo . query 'SourceQuery' --glob '*.rs'
soopy --repo . query 'SourceQuery' --format jsonl
soopy --repo . query 'SourceQuery' --fzf
```

`query` runs `rg`; `--fzf` pipes its result through `fzf`. Neither program is a
library dependency or a Soopy public type.

## Library

### Files and bytes

```rust
use soopy::{Pattern, ReadRequest, Revision, SourceTree};

let repository = soopy::discover(".")?;
let mut tree = SourceTree::open(repository);
let snapshot = tree.snapshot(&soopy::SourceQuery {
    revision: Revision::Worktree,
    patterns: vec![Pattern("**/*.rs".into())],
})?;
let requests = snapshot.files
    .iter()
    .map(|entry| ReadRequest {
        source: entry.source.clone(),
        expected: Some(entry.content.clone()),
    })
    .collect::<Vec<_>>();
let files = tree.read_many(&requests)?;
# Ok::<(), anyhow::Error>(())
```

The snapshot records repository-relative paths, revision identity, content
identity, and byte size. `read_many` checks each request against its expected
content identity before returning bytes.

### Refs and revision graphs

`Refs` enumerates full Git ref names and retains both sides of annotated tags:
the direct tag-object OID and the peeled target OID. `RevisionGraph` delegates
Git graph mechanics to Git plumbing and returns typed, serializable results.

```rust
use std::sync::Arc;
use soopy::{Revision, RevisionGraph, RevisionGraphQuery};

let repository = soopy::discover(".")?;
let graph = RevisionGraph::open(repository.clone());
let result = graph.query(&RevisionGraphQuery {
    repository: repository.identity.clone(),
    resolve: vec![Revision::Named(Arc::from("main"))],
    parents: vec![],
    ancestry: vec![],
    merge_bases: vec![],
    ahead_behind: vec![],
    walks: vec![Revision::Named(Arc::from("main"))],
})?;
# Ok::<(), anyhow::Error>(())
```

One query may request revision resolution, direct parents, ancestry tests,
merge bases, ahead/behind counts, and reachable-commit walks. Result vectors
remain parallel to their request vectors. An unrelated pair has an empty
merge-base vector. Resolution distinguishes present, absent, corrupt, and
shallow-boundary commits.

### Controlled acquisition

`Acquisition` is the only API in Soopy that fetches objects or changes refs.
Every operation is checked against an explicit policy. The default policy
rejects all operations before running a Git process.

```rust
use std::sync::Arc;
use soopy::{
    Acquisition, AcquisitionOperation, AcquisitionPolicy, AcquisitionRequest,
};

let repository = soopy::discover(".")?;
let acquisition = Acquisition::open(repository.clone());
let outcomes = acquisition.execute(
    &AcquisitionPolicy {
        allow_fetch: true,
        allow_tag_fetch: false,
        allow_unshallow: false,
    },
    &AcquisitionRequest {
        repository: repository.identity.clone(),
        operations: vec![AcquisitionOperation::FetchRef {
            remote: Arc::from("origin"),
            name: Arc::from("main"),
        }],
    },
)?;
# Ok::<(), anyhow::Error>(())
```

Supported operations are `FetchRef`, `FetchTag`, `Deepen`, and `Unshallow`.
The full request is validated before the first permitted mutation. Remote and
ref names must identify configured remotes and valid full branch/tag suffixes.
Each `AcquisitionOutcome` carries the original operation and its receipt.
Receipts distinguish policy rejection, existing data, fetched data, deepened
history, completed history, unavailable acquisition, and a repository that
already has complete history. Tag receipts preserve direct and peeled OIDs.

The intended caller sequence is:

```text
RevisionGraph::query
    -> observe Absent or ShallowBoundary
    -> choose an AcquisitionPolicy outside Soopy
    -> Acquisition::execute
    -> RevisionGraph::query again
```

Git owns revision syntax, graph traversal, refs, fetching, and object storage.
Soopy supplies repository-qualified coordinates, typed requests and results,
validation, batching, content checks, and filesystem/ref change reporting.
