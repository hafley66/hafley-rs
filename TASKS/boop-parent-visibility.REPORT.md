# Report: boop parent visibility

- [What shipped](#what-shipped)
- [The delivery ladder](#the-delivery-ladder)
- [Field incident 01:12:28](#field-incident-011228)
- [Verb audit](#verb-audit)
- [Live end-to-end](#live-end-to-end)
- [Validation](#validation)
- [Left undone](#left-undone)

## What shipped

| # | commit | what it fixes |
|---|---|---|
| 0 | 8c33671 | the supervisor reports every turn end and every commit |
| 1 | 5230ee6 | the delivery ladder and the transition ledger |
| 2 | 59582fa | opencode parts keep their bodies |
| 3 | 05f72c0 | every help example runs through the installed parser |
| 4 | 82dcb50 | a door harness is held, never pasted at; the reconciler ages out |
| 5 | 2984d35 | `boop push`, the send and the wait in one verb |
| 6 | c1963e1 | `boop debug <lane>` answers what happened, in five sections |
| 7 | c13c753 | the verb audit, and the folded verbs hidden |
| 8 | 7873f40 | three defects the live e2e turned up |

Branch state at handoff: the prior worker's `yielded/039a729` and its rework
snapshot `f240599` both proposed an `agent_delivery_transition` table. 039a729
kept a typed `DeliveryState`; the rework wired the table into `record_delivery`
and repointed `delivery_rows` at it, and had already been swept onto `main` by
commit 70b6dc1. This branch keeps the rework's wiring, restores 039a729's typed
vocabulary and read API, and repairs two defects the rework carried: a doc
comment that had swallowed `EdgeRow`'s derive, and a `lane_supervisor` branch
that skipped the landing transition for exactly the routes whose rows went
missing. Both snapshots are pinned as tags `yielded/039a729` and
`rework/f240599`.

## The delivery ladder

Every send path walks one ladder and stops at the first rung that takes the
row. There is no refusal: the last rung is the mailbox itself.

| rung | condition | transition recorded | acks the row |
|---|---|---|---|
| door | a live door session takes the text into the running turn | accepted-by-harness | yes |
| acpx queue | the caller drives the recipient's own queue | accepted-by-harness | yes |
| turn boundary | the recipient's supervisor holds it, or a door harness whose door answered nothing holds it for its next turn | held-for-turn-boundary | no |
| hook inbox | the recipient's project carries an installed inbox hook | queued-in-hook-inbox | no |
| pane paste | the route owns no door at all and names a live pane; a notice is pasted, never the body | pasted-into-pane | no |
| mailbox | nothing above answered; the row waits and the supervisor retries it | held-in-mailbox | no |

The sender prints one line naming the rung and exits 0. The one non-zero exit
left is a message that carries `appended` and nothing else one POLL later,
which means a store the sender cannot write.

Rung 4 goes through the `PanePaster` seam, so a test never touches a real
terminal.

## Field incident 01:12:28

Three rows a previous supervisor run had written to `codex-0` were
re-delivered together and typed into the user's live codex TUI pane. Root
cause was two defects, both fixed in 82dcb50.

| defect | fix | test |
|---|---|---|
| every failed door fell through to the pane | a route whose harness owns a door is held for its turn boundary and never pasted at | `a_door_harness_with_no_live_session_is_held_and_never_pasted` |
| the reconciler retried every unlanded row in the mailbox | three cutoffs: this run's start, a 10-minute age, and the recipient's registration stamp | `the_reconciler_skips_rows_a_previous_run_wrote`, `..._older_than_one_bounded_turn`, `..._a_route_that_registered_after_the_row` |

The trigger was mine: `boop db sync create` run from the rebuilt binary against
the live mailbox delivers queued mail as a side effect. Every later run in this
branch used `--mail-dir` under the scratchpad.

## Verb audit

Measured with `--help` from the release binary. Counts before this commit and
after it.

| group | before | after |
|---|---|---|
| top level | 18 | 15 |
| beep | 7 | 6 |
| beep lane | 11 | 9 |
| db | 16 | 7 |
| inbox | 2 | 2 |
| agent | 2 | 2 (1 already hidden) |

No code path was deleted. Every folded verb keeps its implementation and its
parse; it carries `hidden = true` and a note naming the audit.

### Top level

| verb | verdict | the one use no other verb covers |
|---|---|---|
| shell-init | defend | prints the shell functions that route an interactive harness through boop; nothing else emits shell source |
| tui | defend | launches a harness TUI and registers the pane in one step |
| codex | defend | attaches a native codex TUI to a boop-owned managed app-server; `tui` opens an unmanaged one |
| beep | defend | the agent-bus group: harnesses, lanes, mail, processes |
| db | defend | read-only SQL against the store, plus the four projections worth a verb |
| debug | defend | the only verb that answers "what happened to this lane" in one read |
| agent | defend | freshly syncs, then summarizes runtime facts; `db` never syncs first |
| concatmap | **fold** | a DL6 refinement runtime, not the agent bus |
| host | **fold** | a DL6 program calls this on stdin/stdout; nobody types it |
| tell-parent | defend | the caller spells neither end of the edge; `push` needs a route name |
| tell-children | defend | fans one body across every child edge with a per-target line |
| whoami | defend | the identity ladder's own answer, with the rung that resolved it |
| wait | defend | resumes a wait on an id a previous command printed |
| push | defend | send and wait in one verb, with typed exits 0/124/3 |
| inbox | defend | the hook a claude coordinator drains at its turn boundary |
| me | defend | acts on the caller's own conversation: mood and favorites |
| config | defend | the resolved config path, its contents, and the preset table |
| adopt | already hidden | registers an existing pane as a coordinator route |
| follow | already hidden | a coarse-poll sync daemon |
| chat | already hidden | the NDJSON chat-repr projection |

### beep

| verb | verdict | the one use no other verb covers |
|---|---|---|
| beep harness | defend | what each installed harness declares: list and get |
| beep lane | defend | the lane group |
| beep agent | defend | registers and closes a pane-less route |
| beep hail | defend | fire-and-forget send; `push` always blocks |
| beep ps | defend | pid, rss and cpu for one lane's process |
| beep message | **fold** | its one verb, `ack`, is folded with it |
| beep pstree | **fold** | `beep lane list` carries the parent column |

### beep lane

| verb | verdict | the one use no other verb covers |
|---|---|---|
| list | defend | one row per lane with liveness and the parent edge |
| create | defend | worktree, warm start, spawn and route registration in one shot |
| get | defend | the whole route row for one lane |
| patch | defend | rewrites one field of a route without a respawn |
| delete | defend | drops a route, optionally the worktree with it |
| prune | defend | drops every route whose session is gone |
| pane | defend | captures the lane pane's screen; nothing else reads a screen |
| message | defend | the lane's own mailbox, in and out |
| wait | defend | blocks on the lane's result row and exits with its rc |
| run | **fold** | the supervisor's entry point, spawned by `lane create` |
| route | **fold** | `beep lane get` prints the same route row |

### db

| verb | verdict | the one use no other verb covers |
|---|---|---|
| chat | defend | the readable transcript, which is what a parent reads |
| usage | defend | tokens per session and per model |
| price | defend | the price table the usage read multiplies by |
| favorite | defend | user-pinned markdown bodies |
| sync | defend | ingests new transcript bytes; the one write verb |
| sync-cursor | defend | the watermark a sync resumes from |
| status | defend | the store's own size, counts and cost |
| session, turn, touch, command, fetch, skill, pr, span, edge | **fold** | nine one-table dumps; `boop db "SELECT * FROM agent_<table>"` answers each |

### inbox, agent, me, config

| verb | verdict | the one use no other verb covers |
|---|---|---|
| inbox drain | defend | takes delivery of queued mail at a turn boundary |
| inbox hooks | defend | installs and removes that hook in a project |
| agent summary | defend | one synced roll-up of runtime facts |
| agent sessions | already hidden | the native session graph, a `db` read with a sync in front |
| me mood | defend | the rendering one session's mail wears |
| me favorite | defend | pins a markdown body for later |
| config path, show, presets | defend | the resolved path, the loaded document, the preset table |

## Live end-to-end

Two lanes, one per harness, spawned from the worktree's own release binary
with `--mail-dir` under the scratchpad. The installed shim was not touched.
The brief said: three commits, `sleep 5` between them, never run boop.

| receipt | opencode (`deepseek-v4-flash-0731`) | codex (`gpt-5.6-luna`) |
|---|---|---|
| a. commit rows with no tell-parent in the brief | 3 | 3 |
| b. mid-flight hail, rc 0 and a rung line | yes | yes |
| c. idle row after the brief turn | yes | yes |
| d. `boop push --timeout 120` | rc 0 in 3.1s | rc 0 in 14.9s |
| e. `boop debug <lane>`, five sections | yes | yes |
| f. `db chat list` non-empty assistant and tool bodies | yes | assistant yes, tool empty |

### a. commits reached the parent unasked

```text
01:30:08 m-4fa075fe chore-e2e-visibility -> e2e-parent yield | commit chore-e2e-visibility 968c6ad..2fe5d6f dirty=0
01:30:13 m-c4845502 chore-e2e-visibility -> e2e-parent yield | commit chore-e2e-visibility 2fe5d6f..61eaee6 dirty=0
01:30:19 m-b3f95e60 chore-e2e-visibility -> e2e-parent yield | commit chore-e2e-visibility 61eaee6..9400de3 dirty=0

01:36:27 m-aa6a05ce chore-e2e-codex2 -> e2e-parent yield | commit chore-e2e-codex2 968c6ad..3944e61 dirty=0
01:36:44 m-a84becc3 chore-e2e-codex2 -> e2e-parent yield | commit chore-e2e-codex2 3944e61..aea4edd dirty=0
01:37:01 m-32ee4505 chore-e2e-codex2 -> e2e-parent yield | commit chore-e2e-codex2 aea4edd..e3a9955 dirty=0
```

The brief forbade running boop. Every one of these rows came from the
supervisor's own HEAD watcher.

### b. a hail lands, and the ledger says where

```text
$ boop beep hail chore-e2e-codex2 --body "reply with the word pong and nothing else" --from e2e-parent
held m-7cdcdd67 -> chore-e2e-codex2 for the next turn boundary (lane supervisor)
to await the reply: boop wait m-7cdcdd67   (or: boop wait --me &)

$ boop db "select sequence, outcome, detail from agent_delivery_transition where message_id='m-7cdcdd67' order by sequence"
{"sequence":1,"outcome":"appended","detail":"mailbox"}
{"sequence":2,"outcome":"held-for-turn-boundary","detail":"lane supervisor"}
{"sequence":3,"outcome":"claimed-by-supervisor","detail":"parked inbox drain"}
{"sequence":4,"outcome":"submitted-to-harness","detail":"resume turn"}
{"sequence":5,"outcome":"accepted-by-harness","detail":"nextturn"}
```

The opencode lane's hail m-d917130e recorded the same five, in the same order.

### c. the idle row

```text
01:30:20 m-08f70cc6 chore-e2e-visibility -> e2e-parent yield | idle chore-e2e-visibility turn=end_turn head=9400de3 dirty=0
01:37:01 m-807452af chore-e2e-codex2    -> e2e-parent yield | idle chore-e2e-codex2 turn=end_turn head=e3a9955 dirty=0
```

### d. push

```text
$ boop push chore-e2e-visibility --body "append the line 'pushed' to notes.md and commit it" --from e2e-parent --timeout 120
held m-28094061 -> chore-e2e-visibility for the next turn boundary (lane supervisor)
idle chore-e2e-visibility turn=end_turn head=fbca667 dirty=0
boop wait m-28094061
real	0m3.091s
rc=0
```

`fbca667` is the commit that push's own turn made.

### e. debug

```text
== 1 route chore-e2e-codex2 ==
kind      lane
harness   codex
model     gpt-5.6-luna
session   01a0368f-6bed-71b3-bcfb-7859960326fc
cwd       .../scratchpad/e2e-repo/.boop-worktrees/chore/e2e-codex2
parent    e2e-parent
tmux      chore-e2e-codex2
liveness  live
last turn 1787621961319 (8s idle)

== 2 mail chore-e2e-codex2 ==
m-c5dfec2a chore-e2e-codex2 -> e2e-parent [result] held-in-mailbox (route e2e-parent names no harness)
m-7cdcdd67 e2e-parent -> chore-e2e-codex2 [request] accepted-by-harness (nextturn)
m-b5ac479b chore-e2e-codex2 -> e2e-parent [yield] held-in-mailbox (route e2e-parent names no harness)
m-c48aa6d2 e2e-parent -> chore-e2e-codex2 [request] accepted-by-harness (nextturn)
m-75b5b35e chore-e2e-codex2 -> e2e-parent [yield] held-in-mailbox (route e2e-parent names no harness)

== 3 worktree chore-e2e-codex2 ==
e3a9955 note three
aea4edd note two
3944e61 note one
968c6ad base
dirty 1

== 4 transcript chore-e2e-codex2 ==
assistant #19 Created the three commits in order: - `3944e61` note one ...
assistant #21 pong
assistant #26 The line `pushed` is appended to `notes.md`. ...
tool #23
tool #24
tool #25

== 5 alerts chore-e2e-codex2 ==
no warn/error in the last 2m
```

### f. transcript bodies

The opencode lane's session `ses_fc9761ba9ffektF8Jo0MpsnyG2`, projected by
this branch's binary into a scratch store: 15 turns, none empty.

```text
{"turn":2,"role":"assistant","said":"reasoning: Simple task. Let me check the file and append lines, commit"}
{"turn":3,"role":"tool","said":"tool bash\ninput: {\"command\":\"printf 'note one\\n' >> notes.md && git a"}
{"turn":4,"role":"tool","said":"patch 5e8f4ea5147136fb03de5f0bed42ea712dfe721a\nfiles: /private/tmp/cla"}
{"turn":9,"role":"assistant","said":"Done. Three commits created: `2fe5d6f` (note one), `61eaee6` (note two"}
{"turn":11,"role":"assistant","said":"pong"}
{"turn":15,"role":"assistant","said":"Committed `fbca667` (pushed)."}
```

The same session in the live store holds 10 turns with 6 empty, because the
installed 0.0.3 shim projected it and `agent_turn` inserts are
`INSERT OR IGNORE`. The rows will fill in when the shim carries this branch.

### Defects the e2e turned up

| defect | status |
|---|---|
| a lane whose model spelling the harness rejected reported nothing at all | fixed, 7873f40 |
| `db sync create` delivered mail into the live mailbox regardless of `--mail-dir` | fixed, 7873f40 |
| `boop debug <lane>` section 2 listed each row twice, once per ack copy | fixed, 7873f40 |
| the codex transcript adapter projects empty tool bodies (`tool #23`, `#24`, `#25` above) | open; 7.4 named the opencode adapter, and codex needs the same pass |

## Validation

`CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/cargo-target cargo build --release -p boop && cargo test -p boop -p boop-harness -p boop-store -p boop-proc`

Build:

```text
Finished `release` profile [optimized] target(s) in 6.80s
```

Tests, per crate, at this commit:

```text
boop=159 boop-harness=107 boop-store=129 boop-proc=126
total 521 passed, 0 failed
```

Per-commit counts:

| commit | total passed | failed |
|---|---|---|
| 8c33671 | boop-proc 121 | 0 |
| 5230ee6 | 508 | 0 |
| 59582fa | 510 | 0 |
| 05f72c0 | 511 | 0 |
| 82dcb50 | 515 | 0 |
| 2984d35 | 518 | 0 |
| c1963e1 | 520 | 0 |
| c13c753 | 520 | 0 |
| 7873f40 | 521 | 0 |

Inherited red at the branch point: `main` carried
`hail_to_an_adopted_coordinator_with_no_live_session_is_recorded_unreachable`
failing, because 70b6dc1 renamed the ledger's outcome words without updating
the test. 5230ee6 restructured that test around the ladder.

Opencode projection, measured on the real transcript for session
`ses_fc9c70e30ffeSohOF3KWJkJOWp` by syncing into a scratch store:

```text
before   {"role":"assistant","n":55,"empty":48}
         {"role":"tool","n":107,"empty":107}
after    {"role":"assistant","n":106,"empty":1}
         {"role":"tool","n":190,"empty":0}
```

The one remaining empty row is a message whose only parts were step markers,
which is the usage anchor and carries no text by definition.

Help examples, with the known drift restored as a sabotage receipt:

```text
argv: ["boop", "beep", "lane", "wait", "1", "--wait-timeout", "1"]
error: unexpected argument '--wait-timeout' found
test result: FAILED. 0 passed; 1 failed
```

`boop debug <lane>` on a lane with nothing recorded:

```text
== 1 route nosuchlane ==
none

== 2 mail nosuchlane ==
none

== 3 worktree nosuchlane ==
none

== 4 transcript nosuchlane ==
none

== 5 alerts nosuchlane ==
no warn/error in the last 2m
```

`boop tell-parent`, from a route registered for this task:

```text
boop-parent-visibility -> claude-5 (parent from edge)
queued m-69225de7 -> claude-5 (the claude door takes it at the next turn boundary)
m-69225de7
```

The first attempt failed, and that failure was itself a defect:

```text
Error: no parent edge: `claude-5` registered no parent and the registry holds
no single other coordinator to fall back to; respawn with `--parent <route>`
or register one
```

`tell_parent_target` read only the registry row's parent edge, so a process
the spawner had stamped with `BOOP_PARENT` could not reach its own parent.
5230ee6 gives it a third rung, the spawn stamp, between the edge and the
single-coordinator fallback.

## Left undone

| item | why |
|---|---|
| codex transcript tool bodies | the e2e found them empty the way opencode's were. 7.4 named the opencode adapter alone, and the codex adapter needs its own pass with its own fixture |
| `boop db sync create --rebuild` on the live store | fails with `FOREIGN KEY constraint failed`, a pre-existing defect outside this branch's scope; the projection receipts were taken against a scratch store instead |
| bounded turns (`--turn-budget`) | the fourth design bullet. The turn-end row and the reconciler cover the reporting half; splitting a brief into bounded turns is its own change, and both e2e lanes reached a turn boundary within 25s without it |
| the live store's stale empty bodies | `agent_turn` inserts are `INSERT OR IGNORE`, so rows the old binary projected keep their empty bodies until a `--rebuild`, which is blocked by the row above |
