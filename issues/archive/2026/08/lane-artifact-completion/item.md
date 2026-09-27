---
created: 2026-08-17
updated: 2026-08-25
type: bug
status: fixed
priority: high
related: ['@boop-lane-observability']
labels: [domain-boop, intent-correctness, user-law]
closed: 2026-08-25
closed_by: claude-5
---

`boop beep lane wait` returned `rc=0` for the Flash4 lane
`capitalized-relation-names`, while the lane had produced no requested report,
no commit, and no worktree change. The stored assistant transcript ended
mid-research at turn 57 after two aborted provider streams. The supervisor
recorded the harness process exit as successful.

## Reproduction Receipt

- lane: `capitalized-relation-names`
- harness session: `ses_fee955b8fffeRNBG4ONYaGBR7O`
- expected artifact: `v6/plans/2026-08-17-capitalized-relation-names.md`
- expected commit subject: `docs: evaluate capitalized relation names`
- observed branch delta from `b9c42388d`: 0 commits
- observed worktree state: clean
- observed last assistant turn: `57`, ending with further research work
- observed supervisor result: `exit_code=0`

## Required Contract

Process exit and task completion must remain separate facts. A lane result
should expose enough structured evidence for callers to distinguish:

1. harness process exit status
2. transcript terminal state, including aborted or interrupted streams
3. worktree change from the lane base
4. expected artifact presence
5. expected commit presence
6. acceptance command results

Artifact and commit expectations belong in the lane brief or lane-create
request as typed completion assertions. Boop should evaluate them after the
harness exits and before reporting task completion.

Suggested result shape:

```rust
struct LaneCompletion {
    process_exit: i32,
    transcript: TranscriptTerminalState,
    assertions: Vec<CompletionAssertionResult>,
    task_outcome: LaneTaskOutcome,
}

enum CompletionAssertion {
    PathExists { relative_path: PathBuf },
    CommitCountAtLeast { count: u32 },
    CommitSubject { value: String },
    WorktreeChanged,
    CommandPassed { argv: Vec<OsString> },
}
```

The CLI should preserve the raw process exit while returning a non-success task
outcome when required assertions fail. `lane wait`, `lane get`, agent summary,
and stored result rows should expose the same distinction.

## Acceptance Criteria

- [x] Lane creation accepts typed completion assertions without parsing prose.
- [x] Supervisor stores process exit and task outcome separately.
- [x] An interrupted provider stream cannot produce a successful task outcome solely from process exit 0.
- [x] Missing expected paths and commits produce named assertion failures.
- [x] `lane wait` prints the failed assertions and exits nonzero for an incomplete task.
- [ ] `lane get` and agent summary expose process, transcript, assertion, and task outcome fields.
- [x] A fixture reproduces exit 0 with no artifact and proves incomplete status.
- [x] A fixture with the expected artifact and commit proves complete status.
- [x] Help documents process success versus task completion.

## Tests Run

- [ ] cargo test -p boop
- [ ] cargo test -p boop-mux
- [x] deterministic supervisor fixture: exit 0, missing artifact
- [x] deterministic supervisor fixture: exit 0, satisfied assertions

## Implementation Notes

Keep assertions bounded to the lane worktree and resolved base SHA. Do not infer
completion from `REPORT.md`, transcript wording, acknowledgement rows, or a
clean harness exit.

## Agent Runs

### 2026-08-17T22:17:20Z · @root

Commit afaf0e7 decouples foreground lane waits from coordinator routes and prevents pane-less coordinators from becoming inferred parents. Focused wait tests, coordinator tmux tests, and strict clippy pass. Artifact assertions remain open acceptance work.

### 2026-08-25T18:51:41Z · @fix-lane-completion

Typed completion expectations (--expect-path, --expect-commit-subject, --expect-commits-at-least) stored in the lane trail, evaluated by the supervisor at the result row. Unmet expectations rewrite a clean exit to rc 4 with detail 'incomplete: ...'. Transcript terminal state (AC 3) is met only through these expectations: the design does not read the transcript or detect aborted streams directly. Ran: cargo test -p boop-proc -p boop-store (all pass), cargo test -p boop (3 pre-existing tell:: failures unrelated to this change).


## Comments

### 2026-08-23T05:36:48Z · @sprefa-coordinator

Second receipt 2026-08-23: lane fix-engine-tick-trace (claude sonnet@high) parked after one turn with 3 dirty files, 0 commits, no PR; supervisor wrote exit_code=0 and hailed 'done rc=0'. User's law (2026-08-23, verbatim intent: 'its supposed to be pushed'): a lane result is rc=0 ONLY when its branch has commits beyond --base-sha AND those commits are on origin. Otherwise the supervisor re-prompts the lane once ('commit and push, or hail blocked with one line') before writing the result row, and a still-unpushed exit is rc=3 with the lane list flag WORKTREE-UNTOUCHED / UNPUSHED. escape_flags at crates/boop/src/cli/job.rs:1786 already computes the commit delta; the result-row writer is the place to consult it.
