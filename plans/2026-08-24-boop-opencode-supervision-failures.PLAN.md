# Boop OpenCode lane supervision failures

Measured on 2026-08-24 while coordinating an OpenCode lane from
`/Users/chrishafley/projects/hafley-rxjs`.

No boop behavior code changed. This document records the observed lane lifecycle, command
receipts, missing progress delivery, transcript projection, and a repair plan.

## Contents

1. [Test setup](#1-test-setup)
2. [Expected behavior](#2-expected-behavior)
3. [Observed behavior](#3-observed-behavior)
4. [Evidence](#4-evidence)
5. [Failure boundaries](#5-failure-boundaries)
6. [Required diagnostics](#6-required-diagnostics)
7. [Implementation plan](#7-implementation-plan)
8. [Type signatures](#8-type-signatures)
9. [Instance timelines](#9-instance-timelines)
10. [Storage reads, writes, and uniqueness](#10-storage-reads-writes-and-uniqueness)
11. [Definition of done](#11-definition-of-done)

---

## 1. Test setup

Coordinator repository:

```text
/Users/chrishafley/projects/hafley-rxjs
```

Fresh lane:

```text
lane: feature-generic-graph-rxjs-renderers
branch: feature/generic-graph-rxjs-renderers
worktree: /Users/chrishafley/projects/hafley-rxjs/.boop-worktrees/feature/generic-graph-rxjs-renderers
base: 4842f9b8866c5cc408e607e4ff901d703ebd12b7
harness: opencode
model preset: pro4
resolved model: openrouter/deepseek/deepseek-v4-pro-0813
conversation: ses_fc9db2240ffeXH41Dc1Grco8l9
supervisor PID: 77752
```

The lane was created with:

```sh
boop beep lane create \
  --branch feature/generic-graph-rxjs-renderers \
  --brief /private/tmp/hafley-rxjs-generic-graph-brief.md \
  --goal "Implement the generic graph model and RxJS renderer boundary plan, with tests and checkpoint commits" \
  --preset pro4 \
  --base-sha 4842f9b8866c5cc408e607e4ff901d703ebd12b7
```

The dry run correctly resolved `pro4` to the OpenCode harness and printed the expected branch,
worktree, base SHA, parent, and model.

## 2. Expected behavior

The `boop --help` contract says:

1. Lane hails are drained by the supervisor every 700 ms.
2. A hail starts a resume turn when the harness cannot accept it mid-turn.
3. Nothing is dropped.
4. `boop tell-parent --kind yield --body "..."` sends progress to the registered parent.
5. The parent receives completion through its harness door without polling.
6. `boop db chat` reads what the agent did.
7. Liveness requires both a live process and observable worktree changes.

The coordinator sent these progress requirements:

```sh
boop tell-parent --kind yield \
  --body "checkpoint=<sha>; tests=<results>; current=<task>; next=<task>; blocker=<none or text>"
```

The lane was told to send one yield immediately and another after every checkpoint commit,
completed test run, scope change, or blocker.

## 3. Observed behavior

The OpenCode process remained live and changed the worktree. Three commits landed:

```text
672e02a grapht-model: add generic graph model, validation, and indexes
6330971 grapht: add generic geometry, presentation, frame, operator contracts, and header stacking
4fe4266 ingest: lower D2, Mermaid, and filesystem sources into the canonical graph
```

No progress yield arrived at the parent before or after those commits.

The coordinator discovered the commits by running `git log` in the worktree. Boop did not
surface them as progress.

Two progress hails produced delivery records with `queued-for-turn-boundary`:

| message | route | outcome | detail | `at_ms` |
|---|---|---|---|---:|
| `m-656398fb` | `feature-generic-graph-rxjs-renderers` | `queued-for-turn-boundary` | `lane supervisor` | 1787614877996 |
| `m-23989685` | `feature-generic-graph-rxjs-renderers` | `queued-for-turn-boundary` | `lane supervisor` | 1787615625351 |

The records prove queue admission. They do not prove that OpenCode received the instruction,
executed `tell-parent`, or that the parent door received a yield.

## 4. Evidence

### 4.1 Process liveness

`boop beep ps feature-generic-graph-rxjs-renderers` initially returned PID `0`, then later:

```text
lane                                      pid    rss_kb  cpu_pct  uptime_sec  children
feature-generic-graph-rxjs-renderers      77752  686640  0.0      1000        4
```

The first PID `0` result occurred after lane creation reported successful dispatch. `boop debug`
reported no lane alert explaining the transient state.

### 4.2 Route liveness

```text
resolved feature-generic-graph-rxjs-renderers
  -> ses_fc9db2240ffeXH41Dc1Grco8l9 (self-reported)
```

The route existed and identified the OpenCode session.

### 4.3 Worktree liveness

The branch began at `4842f9b`. Git later showed three clean checkpoint commits through
`4fe4266`. This proves the harness was executing repository work while no progress yield reached
the coordinator.

### 4.4 Hail receipts

Both `boop beep hail` calls printed:

```text
delivery: lane supervisor
outcome: queued-for-turn-boundary
```

No receipt exposed these later states:

```text
drained-by-supervisor
submitted-to-harness
accepted-by-harness
turn-started
turn-ended
tell-parent-observed
parent-door-delivered
```

### 4.5 Transcript projection

`boop db chat list --session ses_fc9db2240ffeXH41Dc1Grco8l9 --turn-from 80 --limit 80
--format text` returned turns 80 through 123. Their roles and timestamps existed, but assistant
and tool bodies were empty:

```text
80 tool      1787615375673
81 assistant 1787615378479
...
122 assistant 1787615711030
123 assistant 1787615805602
```

The initial user brief was projected with its body. Later assistant and tool records contained
no readable activity. The transcript reader therefore could not answer what OpenCode was doing,
why it ignored progress instructions, which commands it ran, or whether its test claims were
accurate.

### 4.6 Debug output

`boop debug --since 5m --lane feature-generic-graph-rxjs-renderers --json` returned:

```json
{"alerts":[]}
```

The same response included transcript synchronization passes, including projected OpenCode
rows. No alert covered missing bodies, queued hails without downstream progress, or absence of
parent yields.

### 4.7 CLI contract drift

`boop --help` documented:

```sh
boop beep lane wait <lane>
```

and described `--wait-timeout <s>`. The installed subcommand rejected `--wait-timeout` and
reported:

```text
unexpected argument '--wait-timeout' found
tip: a similar argument exists: '--timeout'
Usage: boop beep lane wait --timeout <TIMEOUT> <LANE>
```

The wait was unnecessary for an ACP parent and was cancelled. The mismatch still makes the help
text an invalid command source.

## 5. Failure boundaries

| boundary | measured result | missing information |
|---|---|---|
| lane creation | worktree and route created | first `ps` returned PID 0 |
| process supervision | PID and children eventually visible | no current turn or activity state |
| hail append | delivery row written | no downstream lifecycle states |
| lane inbox drain | undocumented by receipts | whether either message was drained |
| OpenCode prompt injection | undocumented by receipts | whether OpenCode received either instruction |
| OpenCode execution | Git commits prove work | no readable tool or assistant transcript bodies |
| `tell-parent` | no yield observed | whether command was attempted or failed |
| parent ACP door | no yield observed | whether a parent route/delivery failure occurred |
| diagnostics | no alerts | missing-body and stuck-hail conditions were silent |

The evidence does not isolate one defect. It identifies an unobservable chain with at least four
possible failure sites: inbox drain, OpenCode prompt injection, child compliance, and parent
delivery.

## 6. Required diagnostics

Each message needs a monotonic delivery history keyed by `message_id`:

```text
appended
claimed-by-supervisor
submitted-to-harness
accepted-by-harness | rejected-by-harness
turn-started
turn-ended
reply-appended | no-reply
parent-door-delivered | parent-door-failed
```

Each transition records:

```text
message_id
lane_id
session_id
harness
state
at_ms
error_code?
error_detail?
```

`boop debug` must alert on:

1. A lane route with PID `0` after dispatch grace expires.
2. A queued hail without `claimed-by-supervisor` inside two poll intervals.
3. A claimed hail without harness acceptance or rejection.
4. An OpenCode transcript row whose role is assistant or tool and whose projected body is empty
   while the raw event contains content.
5. A `tell-parent` invocation whose parent route cannot be resolved or delivered.
6. A help example rejected by its own installed parser.

## 7. Implementation plan

### 7.1 Reproduce with a deterministic lane

Create a fixture harness that:

1. Blocks in turn A.
2. Receives one hail during turn A.
3. Emits a known progress body through `tell-parent`.
4. Completes turn A.
5. Exits.

Record every delivery transition and assert its order with one inline snapshot.

### 7.2 Add delivery transition receipts

Persist each lifecycle transition instead of overwriting or summarizing it as one outcome.
Retain `agent_delivery` as an append-only history or add a child transition table keyed by
`message_id` and sequence.

### 7.3 Instrument the lane supervisor

At the 700 ms inbox drain boundary, record:

```text
message observed
message claimed
injection path selected
resume turn requested
resume turn accepted or rejected
```

Surface typed OpenCode errors and session-busy states.

### 7.4 Repair OpenCode transcript projection

Compare raw OpenCode ACP events with `agent_turn` rows for turns 80 through 123. Preserve readable
assistant text, tool name, tool input, tool result, and error content. An empty projected body is
valid only when the raw event is structurally empty.

### 7.5 Trace `tell-parent`

Give `tell-parent` a receipt containing the resolved caller identity, parent route, appended
message ID, delivery outcome, and failure. Make this receipt visible through `boop db` and
`boop debug`.

### 7.6 Add progress supervision

Lane creation may accept a progress contract:

```text
after_commit
after_test
after_scope_change
heartbeat_interval
```

The supervisor can observe Git HEAD changes independently of model compliance. When HEAD moves
without a progress yield, emit a diagnostic naming the old SHA, new SHA, and elapsed time.

### 7.7 Generate help examples from clap

Remove the `--wait-timeout` example or add the alias. Test every command printed in doctrine
against clap parsing.

## 8. Type signatures

```rust
struct DeliveryTransition {
    message_id: MessageId,
    lane_id: LaneId,
    session_id: Option<SessionId>,
    harness: HarnessId,
    state: DeliveryState,
    at_ms: i64,
    error_code: Option<String>,
    error_detail: Option<String>,
}

enum DeliveryState {
    Appended,
    ClaimedBySupervisor,
    SubmittedToHarness,
    AcceptedByHarness,
    RejectedByHarness,
    TurnStarted,
    TurnEnded,
    ReplyAppended,
    ParentDoorDelivered,
    ParentDoorFailed,
}

struct LaneProgress {
    lane_id: LaneId,
    previous_head: Option<CommitSha>,
    current_head: CommitSha,
    last_yield_at_ms: Option<i64>,
    observed_at_ms: i64,
}

fn append_delivery_transition(
    store: &Store,
    transition: DeliveryTransition,
) -> Result<(), StoreError>;

fn project_opencode_event(
    event: &RawHarnessEvent,
) -> Result<Vec<AgentTurn>, ProjectionError>;

fn inspect_lane_progress(
    lane: &LaneRoute,
    git: &dyn GitReader,
    now_ms: i64,
) -> Result<Option<LaneProgressDiagnostic>, ProgressError>;
```

Body pseudocode:

```text
append_delivery_transition:
  validate transition against the previous state for this message
  append one row using message_id plus next sequence
  never replace an earlier transition

project_opencode_event:
  match the raw ACP update variant
  preserve assistant text and tool request/result bodies
  emit a typed diagnostic when a content-bearing event projects to an empty body

inspect_lane_progress:
  read worktree HEAD
  read latest parent yield for the lane
  if HEAD advanced after the latest yield beyond the grace interval, emit a diagnostic
```

## 9. Instance timelines

### Lane

```text
lane create
  -> route registered
  -> supervisor process starts
  -> harness session starts
  -> route self-reports session ID
  -> lane works and commits
  -> completion appended and delivered
  -> route removed
```

### Hail

```text
CLI appends hail
  -> supervisor claims hail
  -> supervisor selects mid-turn or next-turn path
  -> harness accepts or rejects prompt
  -> turn starts and ends
  -> optional reply or yield appended
  -> parent supervisor claims reply
  -> parent harness door accepts or rejects reply
```

### Transcript projection

```text
raw OpenCode event written
  -> sync cursor advances once
  -> adapter decodes event
  -> one or more agent_turn rows appended
  -> chat representation preserves readable content
  -> projection diagnostic emitted if content disappears
```

## 10. Storage reads, writes, and uniqueness

### Delivery transitions

- Write once per state transition.
- Unique key: `(message_id, sequence)`.
- Optional invariant: one terminal accepted or rejected state per injection attempt.
- Read by message ID for `boop wait`, `boop debug`, and delivery inspection.

### Transcript turns

- Raw transcript remains the source of truth.
- Sync cursor advances only after projection writes commit.
- Unique source identity combines harness, transcript path, and raw event position.
- Rebuild reproduces the same non-timestamp content and ordering.

### Lane progress

- Git HEAD is read from the registered worktree.
- Parent yields are read by lane/parent edge and message kind.
- Progress diagnostics are derived. They do not become a second lane state authority.

## 11. Definition of done

1. A deterministic OpenCode fixture receives a mid-run hail and sends a parent yield.
2. The parent receives that yield without polling or an explicit wait command.
3. `agent_delivery` exposes every transition from append through parent delivery.
4. A rejected or unreachable injection identifies the exact failing boundary.
5. `boop db chat` renders non-empty OpenCode assistant text and tool request/results from the
   fixture.
6. A content-bearing raw event cannot silently project to an empty chat body.
7. A lane that commits without yielding produces a diagnostic containing both SHAs.
8. A dispatched lane that remains PID `0` beyond the grace interval produces a diagnostic.
9. `tell-parent` exposes caller, parent, message ID, and delivery receipt.
10. Every shell command printed by `boop --help` passes clap parsing in an automated test.
11. Rebuilding the store reproduces delivery histories and transcript projections.
12. Existing Claude, Codex, Kimi, and OpenCode lane completion tests continue to pass.

