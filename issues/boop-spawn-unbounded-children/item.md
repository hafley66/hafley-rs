---
created: 2026-08-17
updated: 2026-08-18
type: bug
status: done
priority: normal
epic: boop-lane-observability
labels: [domain-boop, intent-correctness]
size: S
closed: 2026-08-18
---

# Three unbounded child processes in the spawn path

## Description

Three children in the spawn path run with no deadline: `just --show`, `just boop-start`, and `sh -c <setup>`. The 10-second law says any single operation over 10s is a defect to investigate, never a budget.

| field | value |
|---|---|
| audit row | section 9, row 8 |
| cost | S |
| needs Chris | no |

Sites:

- `crates/boop/src/worktree.rs:62`, `:83`, `:119`

## Acceptance Criteria

- [x] Each of the three spawns carries a deadline; on expiry the child is killed and the lane fails with a named error.
- [x] The timeout value is one named constant, not three literals.
- [x] Test that a `sh -c 'sleep 999'` setup command fails the spawn within the deadline instead of hanging.
- [x] Nothing seizes the machine: the killed child leaves no orphan (verify the process group is signalled).

## Tests Run

- [x] `cargo test -p boop --no-fail-fast`, lib binary: `test result: ok. 312 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.87s`
- [x] `worktree::tests::a_hung_setup_step_fails_within_its_deadline_instead_of_hanging` ... ok
- [x] `worktree::tests::the_killed_child_leaves_no_orphan` ... ok
- [x] `worktree::tests::a_captured_child_reads_eof_instead_of_a_prompt` ... ok
- [x] `worktree::tests::worktree_spawn_creates_a_branch_at_the_base` ... ok
- [x] `worktree::tests::setup_steps_run_in_order_in_the_worktree` ... ok
- [x] `worktree::tests::main_tree_spawn_refuses_a_non_fast_forward` ... ok

## Implementation Notes

The three children the card names were bounded in PR #20, an ancestor of the
base sha 69f00c1. Closing the card also bounded the spawn path's two git
children (`worktree add`, `merge --ff-only`), which `run_git` still ran through
a bare `Command::output()`, and gave every captured child a null stdin so none
can sit on a prompt the spawn has no terminal to answer.

Source: crates/boop/docs/audit-2026-08-17.md sections 9 and 10 (audit branch `audit/boop-review`, origin/main 49aca76).

Style laws apply: comment budget (no change-log narrative), no `eprintln!` in `src/**` (`tracing` only), no em dashes, banned identifiers `provenance`/`substrate`/`load-bearing`/`regime`.
