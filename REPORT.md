# Soopy source identity contracts (issue 018) report

Implementation lane for `/Users/chrishafley/projects/sprefa-v6/issues/source-identities/item.md`.
Commit: `12b44de`.

Scope follows the Pro4 correction: Soopy owns the stable source-coordinate surface only. Dense
relational IDs (`FileId`, `RevFileId`, `BlobId`, `FileSpanId`) are deferred to
`source-identity-mapping` and were not introduced.

## Type signatures

```rust
pub struct RepositoryId(pub Arc<str>);                 // hash(canonicalize(--git-common-dir))
pub struct WorktreeId(pub Arc<str>);                   // hash(canonicalize(--absolute-git-dir))
pub struct ObjectId(pub Arc<str>);                     // Git object name (hex)
pub struct RepoPath(pub Arc<str>);                     // repo-relative, '/'-separated UTF-8

pub struct RefId {                                     // repo-qualified full ref name
    pub repository: RepositoryId,
    pub name: Arc<str>,
}

pub struct Repository {
    pub root: PathBuf,
    pub identity: RepositoryId,
    pub worktree: WorktreeId,
}

pub enum Revision {
    Worktree,
    Named(Arc<str>),
    Commit(ObjectId),
}

pub enum RevisionId {
    Worktree { worktree: WorktreeId, head: Option<ObjectId>, dirty: bool },
    Commit(ObjectId),
}

pub enum ContentId {
    GitBlob(ObjectId),
    Blake3([u8; 32]),
}

pub struct SourceRef {
    pub repository: RepositoryId,
    pub revision: RevisionId,
    pub path: RepoPath,
}
```

Every public coordinate derives `Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize,
Deserialize`. `Arc<str>` is bridged by the private `arc_str` module in `_0_types.rs` because it has
no derived `Deserialize` (unsized). Serialization uses the plain key (hex string or byte array),
never a `Display` string.

## Lifetimes

| Type | Lifetime |
|---|---|
| `RepositoryId` | stable while the common Git directory path is stable |
| `WorktreeId` | stable while the checkout's Git directory path is stable |
| `RevisionId::Commit` | immutable, object-database lifetime |
| `RevisionId::Worktree` | one observation snapshot lifetime |
| `RefId` | while the repository and ref name both exist |
| `ObjectId` | object-database lifetime |
| `RepoPath` | while the path exists in its repository |
| `ContentId` | content-addressed, content lifetime |
| `SourceRef` | one file placement at one revision |

## Uniqueness rules

| Type | Uniqueness |
|---|---|
| `RepositoryId` | one value per common Git directory |
| `WorktreeId` | one value per checkout root; linked worktrees never share it |
| `RevisionId::Commit` | one value per commit OID |
| `RevisionId::Worktree` | one value per `(worktree, head, dirty)` observation |
| `RefId` | one value per `(repository, full ref name)` pair |
| `ObjectId` | one value per object name |
| `RepoPath` | one value per repository-relative path spelling |
| `ContentId` | one value per byte identity (`GitBlob` OID or `Blake3` digest) |
| `SourceRef` | one value per `(repository, revision, path)` |

`RevisionId::Worktree` carries the checkout `WorktreeId`, so a worktree coordinate cannot alias a
sibling linked checkout. `RevisionId::Commit` stays repository-scoped and is readable from any
linked worktree sharing the object database.

## Changed files

| File | Change |
|---|---|
| `crates/soopy/src/_0_types.rs` | Added `WorktreeId`, `RefId`; added `worktree` to `Repository` and `RevisionId::Worktree`; `serde` derives plus `arc_str` bridge; doc comments for construction/equality/ordering/serialization/lifetime/uniqueness. |
| `crates/soopy/src/_2_repository.rs` | Compute `WorktreeId` from `--absolute-git-dir`; attach to `Repository`. |
| `crates/soopy/src/_3_revision.rs` | Resolve `RevisionId::Worktree` carrying `repository.worktree`. |
| `crates/soopy/src/_7_source_tree.rs` | `read_many` rejects a worktree read whose `WorktreeId` differs from the open checkout. |
| `crates/soopy/Cargo.toml` | Add `serde` with `derive`. |
| `crates/soopy/README.md` | Document the coordinate surface and new invariants 7/10. |
| `crates/soopy/tests/2_identities.rs` | New: five identity tests. |
| `Cargo.lock` | `serde` now a direct dependency of `soopy`. |

## Tests

`cargo test -p soopy` — 22 tests pass (1 unit, 6 pre-existing `0_source_tree`, 10 pre-existing
`1_correctness`, 5 new `2_identities`).

New tests in `tests/2_identities.rs`:

- `linked_worktrees_share_repository_but_have_distinct_worktree_ids`
- `reopening_one_worktree_reproduces_both_identifiers`
- `worktree_source_ref_from_one_checkout_cannot_be_read_through_another`
- `commit_source_ref_is_readable_from_either_linked_worktree`
- `every_public_coordinate_round_trips_without_display_strings`

Consumer parity: `cargo test --test live_hosts` in `sprefa-engine-rs`
(`/Users/chrishafley/projects/sprefa/v6/sprefa-engine-rs`) passes 10/10 with the `soopy` path dep
in `sprefa-engine-rs` and `sprefa-extract` temporarily pointed at this worktree, then reverted.
No consumer source changed; the consumer uses only `discover`, `SourceTree::open`,
`GitFilesQuery`, `Revision`, and `ContentId`, none of which gained fields.

## Verification

- `cargo test -p soopy`: 22 passed, 0 failed.
- `cargo clippy -p soopy --all-targets`: clean, no warnings.
- `cargo fmt -p soopy`: run once. Unrelated pre-existing non-rustfmt formatting in untouched files
  was reverted to keep the diff focused on this issue; the files changed here are rustfmt-clean.

## Deferred (out of scope, later issue slugs)

- `FileId`, `RevFileId`, `BlobId`, `FileSpanId` dense relational IDs: `source-identity-mapping`.
- Ref enumeration and tag crawling; `RefId` is a coordinate only, no traversal.
- Revision graph, file-span storage, and DL6 dense-ID mapping: excluded by the issue boundary.
- The pre-existing `_7_source_tree` <-> `_8_watch` module cycle and `_7`/`_9` -> `_10_path`
  out-of-order numeric dependency predate this issue; repairing them needs a structural split
  (e.g. a shared watcher-construction module), not a renumber, and was left for a later lane.
- Full crate `cargo fmt` normalization (the crate predates rustfmt formatting); only files touched
  by this issue were formatted here.
