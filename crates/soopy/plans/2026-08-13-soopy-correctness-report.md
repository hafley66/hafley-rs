# Soopy correctness (issue 017) report

Implementation lane for `/Users/chrishafley/projects/sprefa-v6/issues/soopy-correctness/item.md`.
Commit: `fa66c52`.

## Changed files

| File | Change |
|---|---|
| `crates/soopy/src/_10_path.rs` | New: `ensure_repository_relative` and `ensure_line_safe` path contracts. |
| `crates/soopy/src/_2_repository.rs` | `RepositoryId` derived from `--git-common-dir`, not per-worktree `--git-dir`. |
| `crates/soopy/src/_3_revision.rs` | `dirty` checks `git status` exit; new `resolve_commit_path`. |
| `crates/soopy/src/_4_worktree.rs` | Strict UTF-8 paths; `WorktreeCache` evicts paths absent from the walk. |
| `crates/soopy/src/_5_git_tree.rs` | Strict UTF-8; drop `120000` symlinks; error on size parse. |
| `crates/soopy/src/_7_source_tree.rs` | `read_many`: root containment, `commit:path` verify, WORK `GitBlob` round-trip. |
| `crates/soopy/src/_8_watch.rs` | `git_dir`+`common_dir`; index classification; linked-worktree ref watches. |
| `crates/soopy/src/_9_git_files.rs` | Strict UTF-8 + line-safe rejection in `listed_paths`; `hash_object` helper. |
| `crates/soopy/src/lib.rs` | Declare `_10_path`. |
| `crates/soopy/tests/1_correctness.rs` | New: ten regression tests, one per corrected finding. |
| `crates/soopy/README.md` | Correctness invariants now stated as enforced, with the rejection contract. |

## Findings resolved

| Finding | Scope item | Resolution |
|---|---|---|
| C2 root escape | 1 | `ensure_repository_relative` rejects `..`/absolute/prefix before `root.join`. |
| C3 commit:path trust | 2 | `resolve_commit_path` resolves `commit:path`; read follows it, expected is checked. |
| H1 WORK id round-trip | 3 | `read_many` computes `git hash-object` when `expected` is `GitBlob`, BLAKE3 otherwise. |
| H2 cache retention | 4 | `cache.files.retain` after each walk; racy-second guard unchanged. |
| H3 watcher coverage | 5 | `is_git_index` + `common_dir`/`refs` watches; index writes force a rescan. |
| H4 linked-worktree identity | 6 | `--git-common-dir`; fixture asserts equal id across linked worktrees. |
| M1 symlink parity | 7 | Skip mode `120000` in `_5_git_tree.rs`, matching the walker. |
| M2 status failure | 8 | `dirty` bails on non-zero `git status`. |
| M3/M4 path determinism | 9 | Strict UTF-8 at every enumeration site; newline/CR rejected in `git_files`. |
| L1 size parse | 10 | Malformed `ls-tree` size is an error, not `0`. |

## Tests

`cargo test -p soopy` — 17 tests pass (1 unit, 6 pre-existing integration, 10 new).

New regression tests in `tests/1_correctness.rs`:

- `repo_path_cannot_escape_its_root` (C2)
- `commit_read_verifies_commit_path_against_expected_blob` (C3)
- `worktree_git_files_round_trips_through_read_many` (H1)
- `worktree_cache_evicts_deleted_paths` (H2)
- `tracked_symlinks_are_absent_from_both_worktree_and_commit` (M1)
- `linked_worktrees_share_repository_identity` (H4)
- `failed_git_status_is_not_a_clean_worktree` (M2)
- `non_utf8_commit_path_is_rejected` (M3, crafted git tree)
- `newline_bearing_path_is_rejected_by_git_files` (M4)
- `watcher_reports_an_index_only_change` (H3)

Consumer parity: `cargo test --test live_hosts` in `sprefa-engine-rs` passes 10/10 against
this worktree's `soopy` (path-dep temporarily pointed at this worktree, then reverted; no
consumer source changed).

## Verification notes

- `cargo clippy -p soopy --all-targets`: clean.
- `cargo test -p soopy --features cli`: fails with "the package does not contain this feature".
  The CLI is a `[[bin]]` built unconditionally; there is no `cli` feature and none was added,
  as it falls outside the ten corrected items. Documenting here rather than inventing a gate.

## Deferred (out of scope, later issue slugs)

- C1 (`repo_files*` cwd pathspec prefixing) lives in the `sprefa-engine-rs` consumer
  (`src/hosts.rs:135`), a separate repository, not the Soopy crate; deferred to the consumer.
- L2 `GitBatch` unbounded allocation from an untrusted header (`_6_git_batch.rs`); low, needs a
  caller-defined size bound.
- L3 `discover` cosmetic handling of a nonexistent file start path.
- L4 `SourceTreeBlobSource::blob` treats poisoned-mutex/read failure as absence (consumer).
- A6 symlink canonicalization in `is_git_ref`/`is_git_path` still follows symlinks; low.
- Worktree identity (`WorktreeId`) distinct from repository identity: no such public type
  exists; `Repository.root` carries the per-worktree root today. Deferred to the ref/tag and
  revision-graph API work explicitly excluded from this issue.
- `rg`/`fzf` subprocesses in `main.rs` and the `SourceSearch`/`SourceSelect` traits: deferred.
