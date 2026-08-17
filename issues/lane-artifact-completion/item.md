---
created: 2026-08-17
updated: 2026-08-17
type: bug
status: open
priority: high
related: ['@boop-lane-observability']
labels: [domain-boop, intent-correctness]
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

- [ ] Lane creation accepts typed completion assertions without parsing prose.
- [ ] Supervisor stores process exit and task outcome separately.
- [ ] An interrupted provider stream cannot produce a successful task outcome solely from process exit 0.
- [ ] Missing expected paths and commits produce named assertion failures.
- [ ] `lane wait` prints the failed assertions and exits nonzero for an incomplete task.
- [ ] `lane get` and agent summary expose process, transcript, assertion, and task outcome fields.
- [ ] A fixture reproduces exit 0 with no artifact and proves incomplete status.
- [ ] A fixture with the expected artifact and commit proves complete status.
- [ ] Help documents process success versus task completion.

## Tests Run

- [ ] cargo test -p boop
- [ ] cargo test -p boop-mux
- [ ] deterministic supervisor fixture: exit 0, missing artifact
- [ ] deterministic supervisor fixture: exit 0, satisfied assertions

## Implementation Notes

Keep assertions bounded to the lane worktree and resolved base SHA. Do not infer
completion from `REPORT.md`, transcript wording, acknowledgement rows, or a
clean harness exit.
