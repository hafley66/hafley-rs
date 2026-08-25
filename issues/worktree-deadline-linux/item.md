---
created: 2026-08-20
updated: 2026-08-25
type: bug
status: wontfix
priority: high
epic: boop-process
closed: 2026-08-25
closed_by: claude-5
---

# Setup-step deadline and process-group kill do not hold on Linux

## Description

Two `worktree.rs` receipts pass on macOS and fail on `ubuntu-latest`. Both are
production behaviour, not test scaffolding, and both are red on `main`: base run
32371247357 on `f3d5123` shows them alongside two others.

## Description

`run_status_with_deadline` is meant to bound a `boop-start` setup step and to
kill the whole process group when the deadline passes. On the runner it does
neither.

| test | asserts | runner result |
|---|---|---|
| `a_hung_setup_step_fails_within_its_deadline_instead_of_hanging` | a 2s deadline over `sh -c 'sleep 999'` returns inside 10s | elapsed `999.00216605s`: the deadline never fired |
| `the_killed_child_leaves_no_orphan` | the grandchild of a killed step never runs | `the grandchild ran after the kill, so the group survived it` |

The first one is also why the `boop-harness` test target takes ~1000s on CI: one
test burns the whole `sleep 999`. Fixing it should take the suite's wall time
down by about 16 minutes.

Receipts, run 32380792479 on `refactor/boop-crate-split`:

```
worktree::tests::a_hung_setup_step_fails_within_its_deadline_instead_of_hanging
  panicked at crates/boop-harness/src/worktree.rs:726:9: 999.00216605s
worktree::tests::the_killed_child_leaves_no_orphan
  panicked at crates/boop-harness/src/worktree.rs:771:9:
  the grandchild ran after the kill, so the group survived it
```

Suspect: the kill targets the child pid rather than a process group the child
actually leads, so on Linux `sh` has already exec'd or forked away from it.
`setsid` at spawn plus `kill(-pgid)` is the shape to check first, against the
"nothing seizes the machine" and "10-second law" rules.

## Acceptance Criteria

- [ ] `a_hung_setup_step_fails_within_its_deadline_instead_of_hanging` passes on `ubuntu-latest`, and the deadline is measured, not assumed.
- [ ] `the_killed_child_leaves_no_orphan` passes on `ubuntu-latest`.
- [ ] The `boop-harness` test target's CI wall time is reported before and after.
- [ ] A fail-first receipt names the platform difference, not just the fix.

## Tests Run

## Implementation Notes
