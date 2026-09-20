---
created: 2026-09-19
updated: 2026-09-19
type: feature
status: open
priority: normal
labels: [boop, lane, intent-correctness]
---

# a lane result row carries its validation run, not just rc=0

## Description

## Description

Three lanes in one session reported `lane <name> done rc=0` and retired 60
seconds later without sending a single line of test output. The third lane's
brief named the previous two and restated the reporting contract in a section
headed READ THIS. It did the same thing.

`rc=0` on a result row means the agent's process exited cleanly. It does not
mean the lane's validation command ran, and it does not mean that command
passed. A coordinator reading the row cannot tell the difference, so every lane
result has to be re-verified by hand before it can be merged.

Session receipts: `chat_log/20260919.1.ryi-lines-flag-and-flash-review-cleanup.md`.

| lane | preset | claimed | actually verified by the coordinator |
| --- | --- | --- | --- |
| chore-flash-review-cleanups | sonnet | rc=0 | full gate, 1 target failed, pre-existing |
| feature-extract-lines-flag | glm53f-omp | rc=0 | full gate green, receipt reproduced by hand |
| improvement-cst-out-of-default | glm53f-omp | rc=0 | full gate green, three receipts reproduced by hand |

Asking harder in prose does not work. The contract needs to be structural.

## The shape

A brief declares its validation command. The lane runs it. Boop attaches the
run to the result row. `rc=0` then carries evidence by construction.

Two candidate mechanisms, pick one after reading the store schema:

1. `lane create --verify "<command>"`: boop runs it itself after the agent goes
   idle and before the result row is written, and stores exit code, duration and
   captured output. The lane cannot skip it because the lane does not run it.
2. the brief declares it and the lane is required to call
   `boop beep lane verify <command>`, which runs it and stamps the row. Lighter,
   but a lane that never calls it is back to the current state, so the result
   row must carry a `verified: no` when it was never stamped.

Option 1 removes the failure mode entirely and costs one extra process run per
lane. Option 2 preserves the lane's control over when to validate and needs the
absent case to be loud.

`sprefa-extract`'s own `ryi rename --verify <command>` is prior art in this
repo for the first shape.

## What the row needs

```
verify_command   <string|null>
verify_exit      <int|null>
verify_duration_ms <int|null>
verify_output    <blob|null>   tail-capped, the last N KiB
verified         <bool>        false when no command was declared or it never ran
```

A result row with `verified: false` should read as loudly in `lane list` and in
the done mail as a nonzero exit does.

## Acceptance Criteria
- [ ] a lane with a declared validation command cannot produce a result row without a verify outcome attached
- [ ] `verified: false` is visible in the done mail and in `lane list`, not only in the store
- [ ] verify output is retrievable by lane through an existing read verb, no raw SQL
- [ ] a failing verify command produces a result row that says so, rather than rc=0
- [ ] integration test through the real binary: one lane whose command passes, one whose command fails, one that declares none
- [ ] the herder skill and the lane brief template are updated to stop asking for receipts in prose

## Tests Run

## Implementation Notes

Related: the lane brief form that worked this session is
`TASKS/lane-extract-lines-flag.BRIEF.md` and
`TASKS/lane-cst-out-of-default.BRIEF.md`. Both declare the exact command in a
"Validation, exact commands" section, which is the string this feature would
lift into `lane create`.
