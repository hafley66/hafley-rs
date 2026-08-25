# soopy

`soopy` supplies one Rust interface for filesystem worktrees and immutable
Git revisions. It enumerates repository-relative paths and folders, assigns
content identities, reads blobs in batches, and reports debounced logical
filesystem and ref deltas. The crate contains no Sprefa types or runtime
assumptions.

## Quick start

Build the CLI and inspect a checkout:

```bash
cargo build -p soopy
cargo run -p soopy -- --repo . resolve WORK
cargo run -p soopy -- --repo . files --revision WORK --glob '**/*.rs'
cargo run -p soopy -- --repo . read --revision HEAD --glob 'src/**/*.rs'
```

Watch filesystem changes or inspect tracked Git state:

```bash
cargo run -p soopy -- --repo . watch --format jsonl
cargo run -p soopy -- --repo . status-metrics
```

The mutation API separates planning from writing:

```rust
use soopy::{
    plan_mutations, CommitEngine, DurableStageStore, SourceRoot, StageRequest,
    StageStore,
};

let target = std::path::Path::new("./checkout");
let mut root = SourceRoot::open_directory(target)?;
let request: StageRequest = serde_json::from_slice(&request_json)?;

let plan = plan_mutations(&mut root, &request)?;
let mut stages = DurableStageStore::open("./target/soopy-stages")?;
let staged = stages.save(plan)?;

println!("{:#?}", staged.previews);

// Call commit only after the application has approved staged.id.
let engine = CommitEngine::open(target, "./target/soopy-commit-state")?;
let receipt = engine.commit(&staged)?;
# Ok::<(), anyhow::Error>(())
```

`StageRequest` contains typed create, replace, move, and delete actions. Planning
reads and validates source content without writing. `DurableStageStore` seals
the plan under its content-derived `StageId`. `CommitEngine` checks the staged
inputs again, writes the approved result, journals progress, and returns an
idempotent receipt.

### DryRun mode

A dry run against a throwaway mirror pays for durability it then deletes. Pair
`InMemoryStageStore` with `CommitEngine::open_dry_run` and the same
`StageRequest` runs the same planning, sealing, preflight, journal, apply and
receipt steps with every device flush dropped:

```rust
let mut stages = soopy::InMemoryStageStore::new();
let sealed = soopy::stage_mutations(&mut root, &request, &mut stages)?;
let stage = soopy::show_stage(&stages, sealed.id)?.expect("stage present");
let engine = soopy::CommitEngine::open_dry_run(mirror, state)?;
let receipt = engine.commit(&stage)?;
# Ok::<(), anyhow::Error>(())
```

Previews, applied operations and resulting bytes match the durable path;
`tests/14_commit_engine.rs` pins that. Never point a dry run engine at a root
a human keeps: an interrupted dry run has no crash guarantee.

### Device syncs

macOS `fcntl(2)` gives three settings and soopy uses all three. `fsync(2)`
hands a body to the device. `F_BARRIERFSYNC` orders everything already synced
on that device ahead of everything after it. `F_FULLFSYNC` drains the device
queue, and the man page states that when it returns, "data that had been
fsync'd on the same device before is guaranteed to be persisted". One flush
therefore settles a whole phase, and the protocol spends fences only where an
order matters:

| step | needs | level |
|---|---|---|
| stage blobs before the manifest naming them | order | fence |
| stage manifest | durability | flush |
| commit payloads before the journal naming them | order | fence |
| journal before any target is touched | order | fence |
| targets before the receipt claiming them | order | fence |
| commit receipt | durability | flush |

Journal removal is deliberately unsynced: `commit` reads the receipt before it
looks for a journal, and `recover` reclassifies a fully applied journal as
done. A state root on another volume cannot be fenced against the target root,
so those two crossings pay a full flush instead.

`device_sync_counts()` returns the process tally by level.
`tests/17_durable_flushes.rs` pins the exact numbers.

One Move plus 26 Replace actions over a 282-file mirror, release build, macOS
APFS, three runs each:

| path | stage | rehydrate | commit | total | fsync / fence / flush |
|---|---|---|---|---|---|
| durable | 19.5 / 11.4 / 11.8 ms | 0.8 ms | 26.0 / 27.0 / 23.1 ms | 46.4 / 39.2 / 35.6 ms | 58 / 4 / 2 |
| dry run | 0.6 / 0.7 / 0.6 ms | 0.005 ms | 4.3 / 4.7 / 4.9 ms | 4.8 / 5.4 / 5.5 ms | 0 / 0 / 0 |

A durable commit engine given `DurableStageStore::blobs_dir` hard-links the
staged payloads instead of writing the same bytes a second time:

```rust
let engine = CommitEngine::open(target, state)?.with_staged_blobs(stages.blobs_dir());
```

```bash
cargo run -p soopy --release --example 7_stage_commit_phases -- --dry-run
```

Run the deterministic mutation and repository-scale gates:

```bash
just test-source-mutations
just perf-source-mutations-planner
just test-soopy-multi-repo-refresh
just perf-soopy-multi-repo-refresh
```

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

WatchQuery
    -> RepositorySnapshot
    -> RepositoryDelta { Ref, Source, Index, Worktree, RescanRequired }
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
| `SourceSpan` | `(SourceRef, start, end)` half-open byte offsets | one byte range in one revision-qualified file | placement lifetime |

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
| Filesystem event backend | `notify` | `notify` |
| Filesystem event normalization | `notify-debouncer-full` | `notify-debouncer-full` |
| Worktree hashing | `blake3` | `blake3` |
| Git trees, blobs, revisions, index, refs, and conflicts | `git` subprocesses | `git` subprocesses |
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

## Watch API

`RepositoryWatcher` owns one opened repository, a `WatchQuery`, one prior
`RepositorySnapshot`, and independent source/ref readers. `WatchQuery` selects
one optional worktree source query, one optional ref query, index observation,
linked-worktree observation, and a validated quiet/max coalescing window.

```rust
use std::sync::Arc;
use soopy::{Pattern, RefQuery, Revision, SourceQuery, SourceTree, WatchQuery};

let repository = soopy::discover(".")?;
let tree = SourceTree::open(repository);
let mut watcher = tree.watch_repository(WatchQuery {
    source: Some(SourceQuery {
        revision: Revision::Worktree,
        patterns: vec![Pattern("**/*.rs".into())],
    }),
    refs: Some(RefQuery {
        repository: tree.repository().identity.clone(),
        namespace: Arc::from(""),
        name: None,
        pattern: None,
    }),
    index: true,
    linked_worktrees: true,
    coalescing: Default::default(),
})?;

for delta in watcher.recv()? {
    println!("{delta:?}");
}
# Ok::<(), anyhow::Error>(())
```

`RefDelta::Changed` carries complete old/new `RefObservation` values, including
direct and peeled targets. `RefDelta::HeadChanged` records symbolic, detached,
and unborn transitions plus the resolved old/new commit targets, independent
of the selected `RefQuery`. `IndexDelta` compares BLAKE3 identities of the logical
`git ls-files --stage -z` entry set and is keyed by `WorktreeId`, so unstaged
source writes do not create index deltas. `WorktreeDelta` compares linked
checkout roots, attachment state, and observed commit OIDs returned by `git
worktree list --porcelain`.

The watcher registers the worktree root, current Git directory, shared Git
directory, shared `refs/`, and, when requested, shared `worktrees/`. Object
and pack churn are ignored. A native overflow or callback error emits
`RescanRequired`, then the deterministic old-to-new delta sequence from a
fresh complete snapshot. `notify-debouncer-full` owns raw event normalization,
rename stitching, and its quiet window; Soopy applies the typed path filter and
retains the public maximum receipt collection. The watcher never mutates Git
state.

Tracked-state observations use Git's index/tree protocols and one persistent
`git hash-object --stdin-paths` worker for regular worktree files, so Git
attributes, CRLF conversion, and clean filters participate in worktree object
identity. A repository-owned byte worker uses `git hash-object --stdin
--no-filters` for symlink target bytes because Git's path worker follows a
link. Plain `DirectoryRoot` snapshots remain BLAKE3-based and never invoke Git.

`SourceWatcher` and the `soopy watch` command retain the source-only surface.
An index-only event maps to its existing `SourceDelta::RescanRequired` result.

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

### Revision-qualified spans

`SourceSpan` carries a `SourceRef` plus half-open `[start, end)` byte offsets.
It is the source coordinate that the DL6 runtime later maps to `rev_file_id`
and `file_span_id`; Soopy does not allocate dense relational IDs or store
text, line, or column values with the span.

```rust
use soopy::{SourceSpan, SpanPositionRequest, SpanTextRequest};

let entry = &snapshot.files[0];
let span = SourceSpan {
    source: entry.source.clone(),
    start: 0,
    end: 8,
};
let text = tree.span_text_many(&[SpanTextRequest {
    span: span.clone(),
    expected: Some(entry.content.clone()),
}])?;
let positions = tree.span_position_many(&[SpanPositionRequest {
    span,
    expected: Some(entry.content.clone()),
    newline_index_byte_budget: 1_048_576,
}])?;
# Ok::<(), anyhow::Error>(())
```

`span_text_many` and `span_position_many` batch through `read_many`, including
the persistent `git cat-file --batch` reader for committed blobs. Every range
is validated against retrieved byte length. Positions use one-based lines and
zero-based byte columns, so positions remain defined at arbitrary byte
boundaries, including UTF-8 interiors, empty spans, and EOF. Position requests
must budget temporary line-start storage as `(newline count + 1) *
size_of::<usize>()`.

Current span retrieval is worktree and Git-blob retrieval through `SourceTree`.
Stored-content retrieval is introduced at the downstream `023a` relational
identity/runtime boundary, where stored bytes receive a `ContentId` and map to
`rev_file_id` rows.

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

## Scale receipts

### Producer edit receipts

`ProducedEdit` is the producer boundary for byte and UTF-8 replacements. It
stores an `ActionSpan`, replacement bytes, and one or more rule-bearing
`ActionProducer` records. `ProducedEditBatch` is the lossless planner input;
its `into_text_edits` expansion retains equivalent producer records while
leaving conflicting replacements separate. `from_text_edit` and
`from_utf8_text_edit` are executable adapters over Soopy's existing byte and
UTF-8 shapes. `deduplicate_equivalent_edits` groups identical range and
replacement pairs while retaining every producer and rule. Overlaps and
different replacements remain separate for planner conflict handling.

The ast-grep adapter is the dependency-free `from_ast_grep_parts` function.
An integration extracts scalar fields from the real
`ast_grep_core::source::Edit<S>` value and supplies source identity and its
rule-bearing `ActionProducer`. Biome has the explicitly named
`BiomeBatchMutationContract` conversion seam; no Biome language runtime is
included in Soopy. The 100k conversion smoke is run with:

```bash
cargo run -p soopy --example 1_edit_producers_scale -- --edits 100000
```

The root `justfile` runs the release-built scale harness against an existing
checkout. Receipts are written below ignored `target/soopy-scale/`. JSON keeps
retained-handle construction, tracked-file enumeration, cold batched blob
reads, and warm reads through the same persistent Git batch process separate.
It samples the harness process RSS after each stage and counts open descriptors
before and after retaining the requested handles.
The adjacent resource receipt records peak RSS and operating-system process
counters from `/usr/bin/time`.

```bash
just soopy-scale-linux-deps
just soopy-scale-linux-all
just soopy-scale /path/to/repository ':(glob)**/*.rs' 500 16 local
```

The dependency recipe walks build-description files. The all-files recipe
reads every tracked blob twice and is operator-triggered because repository
size controls its duration and I/O volume. Repeating one repository handle 500
times measures retained Soopy state. It does not model 500 distinct watcher
registrations or 500 distinct repository contents.

Blob answers are owned byte buffers. Batch size therefore bounds live answer
bytes, while the process allocator may retain freed pages after large or
irregular blobs. The Linux recipes default to 16 files per read batch so the
receipt exposes that retained-memory behavior without combining hundreds of
large generated headers into one live answer vector.
