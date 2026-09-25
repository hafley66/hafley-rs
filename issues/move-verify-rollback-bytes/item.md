---
created: 2026-09-25
updated: 2026-09-25
type: bug
status: open
priority: normal
related: ['@ryi-cli-cleanup']
labels: [extract]
---

# ryi move verify rollback: moved bytes not restored; identical re-run refused

## Description

## Description

Two defects after `ryi move ... --commit --verify CMD` fails and rolls back.

1. FIXED on ryi/cross-crate-move: the rollback moved the file back but left its respelled bytes. `VerifyJournal::restore` built its byte-restore stage while the moved file still sat at its new path, so the read of the old path failed and the restore was skipped. Repro (fixtures/rust_cross): `ryi move alpha/src/shapes.rs beta/src/shapes.rs --commit --verify false` -> `rolled back 7 files`, then `git status` showed ` M alpha/src/shapes.rs` (`use crate::base::Unit;`). The restore now reads the bytes where the file lives before the move-back.

2. OPEN: re-running the identical move with the same `--state` is refused:

```
commit refused: receipt 40638f8b...626683f diverged at Directory { path: RootPath("alpha/src/lib.rs") }: post-commit identity does not match receipt
```

Stage ids are content-addressed, so the identical plan reproduces the committed stage id, and soopy's durable store (`soopy/src/_7f_commit.rs:1422`) checks the old receipt against the rolled-back tree. A fresh `--state` works.

## Acceptance Criteria
- [x] a failed verify leaves the tree byte-identical to before the run
- [ ] the identical re-run after a rollback commits with the same `--state`

## Tests Run
- crates/sprefa-extract/tests/173_move_cross_crate.rs (verify-fail rollback leaves a clean tree)

## Implementation Notes
Seen by milestone M5. Part 2 lives in soopy's durable stage store.
