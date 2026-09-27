---
created: 2026-09-25
updated: 2026-09-26
type: improvement
status: open
priority: normal
epic: ryi-new-verbs
related: ['@ryi-cli-cleanup']
labels: [extract]
---

# read side off soopy: worktree via ignore/globset, commits via git CLI batch

## Description
Reads in hafley_scm and ryi's read verbs go through soopy. Split by source:

- Worktree (fast, slow, query, Inputs expansion): no git at all. `ignore::WalkParallel` + `globset` for listing, `std::fs::read` / `memmap2` for bytes. The perf branch already reads fast's inputs straight from disk and made soopy's discover subprocess-free.
- Commits (graph --at/--compare, diff, query --digest, SCIP documents at a revision): git CLI stays the backend, per crates/soopy/plans/2026-08-14-git-optional-watch.md:307-309 (no gix, no git2; git CLI is faster at walking blobs and a git library splits the dependency graph). One long-lived `git cat-file --batch` plus one `git ls-tree -r` per revision, called directly or through soopy's GitBatch.
- soopy keeps edit staging/commit/receipts and its watch core.

## Acceptance Criteria
- [x] `Inputs` directory and glob enumeration uses `ignore::WalkParallel` plus `globset`, without `soopy::discover` or `SourceTree::enumerate`.
- [ ] All worktree read paths make zero git subprocess calls and no soopy calls, including `root()` and `git_root_of_cwd()`.
- [ ] commit read paths spawn at most one cat-file --batch and one ls-tree per revision
- [ ] release timings via hafley-observe chrome trace for Inputs expansion and --at revision read, before/after
- [ ] cargo test --features cli green; ratchet 170 unchanged
- [ ] Move commit enumeration to the planned direct Git CLI batch path in its follow-up card.

## Tests Run

- [x] `ryii fast ../../crates/soopy/src` stdout captured before and after was byte-equal (3,867,401 bytes).
- [x] `scripts/ryi-e2e.sh /tmp/ryi-read-side-bin`: 14/14 checks passed.
- [x] `cargo nextest run --features cli -j 2 --test all -E 'test(/^t_177_crate_scope::/)`: 1 passed.

## Implementation Notes
Blocked by M6 (read side into hafley_scm).

## Repro receipt

2026-09-26: before/after stdout for `ryii fast ../../crates/soopy/src` compares byte-equal at 3,867,401 bytes; `scripts/ryi-e2e.sh /tmp/ryi-read-side-bin` reports 14/14. `Inputs` worktree enumeration now uses `WalkParallel` and `globset`; commit enumeration and other worktree read paths remain unchecked above.
