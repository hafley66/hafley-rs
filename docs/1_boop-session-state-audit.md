# Boop / Instant session state audit

[Open the D2 diagram gallery](4_boop-state-graphs/index.md).

Snapshot: 2026-09-13. Source baseline: hafley-rs `2376a81fb137e58e596511e2f5e08417f845fa2f`; Instant `9fca010e3eca040aff367e258a9eaa65bd80b575`.

This document tables the process, native conversation, trace, route, supervisor, and UI state paths read from these checkouts. Pending worktree behavior is identified separately. It describes code, without claiming that lifecycle tests or the installed applications were verified in this audit. No merges, installations, process restarts, or live-store writes were performed.

## Recovered working session

The September 12 Codex session **Fix boop fork TUI pane**, ID `01a095fa-7699-77c0-acc1-323647021271`, ended with the user saying `wait stop i have work in others we done here?`. The final answer recorded no merges and delivery heads Boop `b52826f`, Instant `e9241fae`. Both remain outside their respective main branches. It recorded 13 Boop and 6 Instant tests passing in that turn; those are historical receipts, not new test results.

Transcript: `/Users/chrishafley/.codex/sessions/2026/09/12/rollout-2026-09-12T10-16-54-01a095fa-7699-77c0-acc1-323647021271.jsonl`.

Earlier relevant saved context: [August 30 session](../chat_log/20260830.0.instant-turn-attribution-tmux-selection-titles.md), [lifecycle consolidation brief](../TASKS/boop-lifecycle-consolidation.BRIEF.md). The August report's claim that `record_status` had no production callers is superseded by current `control.rs` and `sync_session_with`.

## Identity and storage model currently implemented

| Entity | Identity / uniqueness | Stored facts | Lifetime / ownership |
|---|---|---|---|
| OS process | PID in current liveness checks | `ProcessInfo` includes PID, parent PID, start time, command, cwd | One OS process; persisted session binding retains PID but omits process start time |
| tmux transport | Route contains a string naming session, target, or `%pane` | Route `tmux`, `agent_live.tmux_pane_id` | Pane/session lifetime; socket identity is passed separately and is absent from persisted binding |
| Native conversation | `dict_session.value UNIQUE`; `agent_session.session_id PRIMARY KEY` | Harness, cwd, started time; turns/usage keyed by session | Can survive process exit and resume. Storage key uses bare session ID; graph exposes `(harness,id)` |
| Boop trace | `agent_trace.trace_id PRIMARY KEY` | First attached root session, started time | Groups sessions across replacement; no trace lifecycle/status column |
| Trace membership | `agent_trace_span.session_id PRIMARY KEY` | One trace, attachment rule and timestamp | First attachment wins via `INSERT OR IGNORE`; no end time or replacement edge |
| Lane spawn | `agent_lane.spawn_id PRIMARY KEY` | Lane name, trace, parent lane, cwd, harness, branch, model, spawned time | Repeated lane names can have distinct spawn rows |
| Route | Registry map keyed by lane name | Kind, parent, selected native session, tmux, transport, model, registration time | Mutable routing record; not a process identity or turn state |
| Current liveness report | `agent_live.session_id PRIMARY KEY` | Nullable PID/pane/status plus door | Latest accepted observation, not a fresh OS probe |
| Liveness interval | `(session_id,from_ts) PRIMARY KEY` | Status, PID, pane, optional `to_ts` | `record_status` closes/opens intervals when the tuple changes |
| Supervisor residency | JSON map keyed by lane name | `live`, `idle`, `retired` | Supervisor writes to `lane-residency.json`; independent of `agent_live` |
| Native parent relation | `agent_edge` | Parent session, child session, edge kind, first/last timestamp | Graph family traversal follows `spawned`; trace membership is a separate relation |
| Turn execution | Channel instance plus turn events | `Started`, `Done`, `Failed`, `Flaked`; optional receipt | One supervised turn; successful completion may leave the process parked |

Sources: [schema](../crates/boop-store/src/ident.rs), [process facts](../crates/boop-store/src/proc.rs), [runtime types](../crates/boop-store/src/runtime.rs), [route kinds](../crates/boop-store/src/bus.rs), [channel events](../crates/boop-acp/src/channel.rs).

## What “active” currently means

| Consumer / field | Exact predicate | What it measures |
|---|---|---|
| Boop `AgentSummary.active_agents` | `tmux == Live OR process == Live` | Runtime presence, including a parked process |
| Boop `ProcessLiveness` | Recorded PID exists in supplied process snapshot | PID presence; start-time identity is not compared |
| Boop `TmuxLiveness` | `live_sessions.has(target.split(':').first())` | Session-name membership. A `%pane` is not resolved here |
| Boop runtime graph shell state | Either liveness Live -> `live`; otherwise either Dead -> `dead`; otherwise stored status or `unknown` | A precedence fold across process and tmux observations |
| Boop graph `%pane` correction | `target_alive(socket,pane)` -> force shell `live` | Additional exact pane probe after runtime snapshot |
| Boop graph native-session state | SQL joins `agent_live.status_id` | Durable report; runtime merge updates shell nodes, not these native nodes |
| Boop CLI `lane list` | Parent-hop / tmux / residency rules below | A separate classifier |
| Instant graph local `active` | `(last_activity_ts ?? started_ts ?? 0) >= sinceTs` | Recency cutoff |
| Instant Boop panel “active only” | Node state `live` OR any descendant passes recursively | String-state filter including ancestors of live descendants |
| Instant tmux attached dot | `session_attached != 0` | A tmux client is attached |
| Instant selected title | `window_active == 1 AND pane_active == 1` | Which pane supplies the displayed title |
| Instant tmux `open` | Frontend tab state | Whether Instant opened the session in a tab |

Sources: [summary.rs:146](/Users/chrishafley/projects/hafley-rs/crates/boop-store/src/summary.rs:146), [runtime.rs:330](/Users/chrishafley/projects/hafley-rs/crates/boop-store/src/runtime.rs:330), [_0_session_graph.rs:434](/Users/chrishafley/projects/hafley-rs/crates/boop-store/src/_0_session_graph.rs:434), [0_boopGraph.ts:110](/Users/chrishafley/projects/instant/src/0_boopGraph.ts:110), [boopPanel.tsx:266](/Users/chrishafley/projects/instant/src/boopPanel.tsx:266).

## Native wrapper transition table on main

`bind_native_session(store, route, trace, session, pid) -> Result<()>`

`apply_native_event(store, route, trace, event, pid) -> Result<()>`

`release_native_route(store, dir, name, route, pid) -> Result<()>`

| Event / condition | Before | Writes / next state | Identity effect |
|---|---|---|---|
| Launch, native session unresolved | Route has no selected session | Discovery retries for up to 10 s; unresolved remains nullable | PID may exist before conversation binding |
| Unique PID match in native registry | Unbound | Select matched session, substituting its parent session if supplied | Exact PID evidence selects conversation |
| Native pane relation found | Unbound | Select session from adapter pane relation | Pane binding supplies conversation |
| Adapter registry lacks process names | Unbound | Choose most recently observed session in canonical cwd, observed at/after launch | Timestamp/cwd fallback; concurrent same-cwd ambiguity is not explicitly represented here |
| First observed `Session(S)` | No selected session | Attach S to existing trace of S, else carried trace, else `trace-S`; write S `live`, PID, pane | New binding; first trace attachment wins |
| Repeated `Session(S)` | S selected | Reapply binding and settings | Same trace and session; repeated identical status tuple adds no interval |
| Observed `Session(B)` replaces A | A selected | A -> `detached`, PID/pane cleared; bind B -> `live` | Same wrapper PID may now carry B; existing trace of B takes precedence over carried trace |
| Current-session `Settings` | S selected | Replace route model; set or clear effort attribute | Session, process and trace unchanged |
| Late settings/close for another session | B selected, event names A | Ignore event | B remains selected |
| Current-session `Closed(S)` | S selected | S -> `closed`, PID/pane cleared; route session/model cleared | Conversation remains stored; wrapper may continue |
| Observer `Failed` | Any observed binding | Return error | Observer failure has no dedicated durable state value |
| Wrapper release, PID still owns session row | S bound to this PID | S -> `detached`; clear door | History/trace retained |
| Wrapper release after concurrent rebind | S row now carries another PID | Do not detach new PID; registry transport removal also compares socket/session/registration | Protects the later owner, using PID equality rather than process incarnation |
| Respawn allowed | Observer/backend failure or nonzero numeric exit; uptime >=10 s; retries <3 | Wrapper can retry launch | Process may change while native conversation is resumed |
| Clean exit, signal death, short uptime, exhausted cap | Respawn predicate false | End wrapper | Historical native conversation is still resumable |

Sources: [control.rs:37](/Users/chrishafley/projects/hafley-rs/crates/boop/src/cli/control.rs:37), [control.rs:176](/Users/chrishafley/projects/hafley-rs/crates/boop/src/cli/control.rs:176), [detach_process:2417](/Users/chrishafley/projects/hafley-rs/crates/boop-store/src/ident.rs:2417).

## Clear / exit / resume / compact identity table

These are the implemented binding rules and lifecycle-test assertions. Harness-native behavior is not established by a comment saying an ID changes.

| Operation | Process | Native conversation | Boop trace | Represented transition / limitation |
|---|---|---|---|---|
| Fresh launch | New PID | Newly observed S | Existing trace(S), otherwise carried trace, otherwise `trace-S` | Launch is unresolved until discovery supplies S |
| Clear/new, replacement observed | May retain PID | A -> B | Carried trace when B has no prior attachment | Old A detached; B live; test asserts B differs and trace stays |
| Clear command with no observed replacement | Can remain alive | Binding remains previous/nullable according to events | No justified new attachment | No explicit `clearing` or `binding_pending` state |
| Compact | May retain PID | Lifecycle gate asserts same session | Same membership | No `Compacted` variant in main `NativeTuiEvent`; gate counts native transcript compact receipts |
| Exit | Old process ends | Stored S remains | Membership remains | Wrapper release writes detached if it still owns S |
| Resume S in another process | New PID | Same S | First membership retained | Gate asserts same session, same trace and changed PID |
| Resume an already-known different session B | Current process can switch | B selected | Existing trace(B) takes precedence | Can switch trace rather than merge histories |
| Resume after clear | New process | Cleared-to B resumed | Trace(B) retained | Covered as a lifecycle gate scenario |
| Concurrent unrelated launches in same cwd | Distinct processes | Distinct observed sessions | Gate asserts distinct traces | Cwd-only fallback remains an independent collision risk |
| PID reused by OS | Numerically same PID, different lifetime | Old stored binding can remain | Unchanged | Current liveness lacks start-time comparison |

Sources: [lifecycle gate:956](/Users/chrishafley/projects/hafley-rs/crates/boop/tests/4_lifecycle_gate.rs:956), [compact assertion:1167](/Users/chrishafley/projects/hafley-rs/crates/boop/tests/4_lifecycle_gate.rs:1167), [clear assertion:1320](/Users/chrishafley/projects/hafley-rs/crates/boop/tests/4_lifecycle_gate.rs:1320). These tests were read, not run during this audit.

## Durable observation reducer

`record_status(session, ts, status, pid, pane) -> Result<()>`

| Input | Effect |
|---|---|
| Timestamp older than current interval start | Ignore observation |
| Same status/PID/pane tuple | Update current cache; no new interval |
| Changed tuple, later timestamp | Close previous interval at ts; open new interval at ts; replace current cache |
| Changed tuple, same timestamp | Upsert same `(session,from_ts)` interval; intermediate same-millisecond state is overwritten |
| Any status string | Intern into `dict_status`; no enum-based transition validation |
| Transcript sync with PID, pane, or no existing live row | Write `live` if transcript session has tmux, otherwise `idle` |
| Transcript sync without PID/pane and existing live row | Preserve existing live row |

Consequently, stored `idle` can mean “transcript discovered without tmux.” Native `LiveStatus::Idle` separately means the adapter observed an idle harness. Supervisor residency `idle` separately means its channel is parked between turns.

Sources: [record_status:2702](/Users/chrishafley/projects/hafley-rs/crates/boop-store/src/ident.rs:2702), [sync_session_with:3443](/Users/chrishafley/projects/hafley-rs/crates/boop-store/src/ident.rs:3443), [LiveStatus:15](/Users/chrishafley/projects/hafley-rs/crates/boop-harness/src/live.rs:15).

## Supervisor transition table on main

| Event / guard | Prior state | Next action / state | Separate durable effects |
|---|---|---|---|
| Start a turn | Newly opened or parked | Residency `live`; channel starts turn | Trace turn-start event |
| Channel `Started` | Turn running | Continue polling | Start acknowledgement handling |
| No event | Turn running | Continue polling, parent/stall checks | Silence alone does not mean completion |
| Delivery deferred to next turn | Running | Add to held messages | Delivery transition records |
| `Done` with pending messages | Running | Drain held messages into next turn | Turn-finish `completed`; completion result deferred |
| `Done`, no held messages, completion verdict available, no prior result | Running | Write result once | Result can precede supervisor exit |
| `Done`, no held messages | Running | Residency `idle`; park on mailbox | Channel remains open |
| Mail arrives while parked | Idle | Next turn, residency `live` | Claim queued messages |
| `Flaked`, retry budget remains | Running | Increment retry counter; continue with resume text | Retry notification; retryable turn finish |
| `Flaked`, exhausted, no held messages | Running | Close channel and return Ended | Failure result/notification path |
| `Failed`, no held messages | Running | Close channel and return Ended | Failure result |
| Non-success with held messages | Running | Held-message branch remains eligible | Code's exit guard explicitly requires `held.is_empty()` |
| Idle eviction predicate | Idle | Close channel, residency `retired`, return retired Ended | Retirement trace/result path |
| Parent death, Kill policy | Running or parked | Close/end via parent watch | Parent-died result |
| Parent death, Reparent policy | Running or parked | Resolve replacement coordinator, rewrite parent when available | Parent relation update |
| Parent death, Orphan policy | Running or parked | Continue with dead parent relation | Process may remain alive |
| Stall timeout during running turn | Running | Failure/recovery path | Default configured limit 30 minutes; idle park does not use this check |
| Explicit delete / termination signal | Running or parked | Signal/cleanup path ends supervisor | Result/trail write where handler runs |

Sources: [supervise.rs:1204](/Users/chrishafley/projects/hafley-rs/crates/boop-proc/src/supervise.rs:1204), [supervise.rs:1450](/Users/chrishafley/projects/hafley-rs/crates/boop-proc/src/supervise.rs:1450), [supervise.rs:1530](/Users/chrishafley/projects/hafley-rs/crates/boop-proc/src/supervise.rs:1530), [parent policy](../crates/boop-store/src/session.rs).

## CLI lane-state precedence on main

Evaluate top to bottom in `lane_state_hop`:

| Condition | Returned state |
|---|---|
| Pane-less coordinator/native, second parent hop | `?` |
| Pane-less coordinator/native, no parent | `?` |
| Pane-less coordinator/native, parent named but missing from registry | `live` |
| Pane-less coordinator/native, registered parent | Classify parent with one-hop bound |
| No tmux listing available | `?` |
| Target not alive | `dead` |
| Target alive and residency idle | `idle` |
| Target alive and residency retired | `retired` |
| Target alive, any other/missing residency | `live` |

Source: [job.rs:2631](/Users/chrishafley/projects/hafley-rs/crates/boop/src/cli/job.rs:2631). This main implementation does not perform the runtime projection's independent PID check.

## Delivery states are a separate axis

`DeliveryState` declares: `Appended`, `ClaimedBySupervisor`, `SubmittedToHarness`, `AcceptedByHarness`, `RejectedByHarness`, `HeldForTurnBoundary`, `QueuedInHookInbox`, `PastedIntoPane`, `HeldInMailbox`, `TurnStarted`, `TurnEnded`, `ReplyAppended`, `NoReply`, `ParentDoorDelivered`, `ParentDoorFailed`, `CooledOff`.

These describe individual mail attempts/receipts. They do not establish whether the native process is alive or whether all turns on a trace are finished. Source: [ident.rs:288](/Users/chrishafley/projects/hafley-rs/crates/boop-store/src/ident.rs:288). Full delivery routing and retry-budget behavior is outside the process/session transition tables above.

## Screenshot and Instant projection

| Surface | Code path | Data lost or folded |
|---|---|---|
| Screenshot session/title/proc/win grid | `pty::list_sessions` -> `session_pane_info` -> `TmuxRow` -> `TMUX_COLUMNS` | `Session` / `TmuxRow` have no process-liveness, turn-state, trace, native-session, or `pane_dead` field |
| `proc` cell | Foreground command; if `boop`, attempt descendant command lookup | A command label does not carry lifecycle evidence |
| `[dead]` in title | Title comes from tmux pane/window text | Table does not parse that text into a typed dead state |
| Boop network graph | `read_session_graph` -> runtime graph -> `buildGraphTree` | Session state and shell state come from different classifiers |
| Current main native call | Query `tmux: None`, runtime `tmux_socket: None` | Global query/default socket; terminal scoping work is pending on another branch |
| Active-only filter | Recursive `state === 'live'` | Does not show why a row is live, unknown, or recently observed |
| Historical finish time | SQL maximum interval start with status `dead` | Wrapper exit writes `detached`, explicit close writes `closed`; those do not populate this `finished_ts` calculation |

Sources: [pty.rs:188](/Users/chrishafley/projects/instant/src-tauri/src/pty.rs:188), [tablepanels.tsx:16](/Users/chrishafley/projects/instant/src/tablepanels.tsx:16), [0_boop.rs:1505](/Users/chrishafley/projects/instant/src-tauri/src/0_boop.rs:1505), [_0_session_graph.rs:169](/Users/chrishafley/projects/hafley-rs/crates/boop-store/src/_0_session_graph.rs:169).

## Pending implementation that changes these tables

| Worktree / branch | Compared with main | Relevant behavior |
|---|---|---|
| `fix/f41-visible-tui-20260912`, `b52826f` | 21 commits ahead, 0 behind; clean | Adds native lane TUI channel and receipts; commit `23dd3f2` changes native completion to require harness idle on its polling path |
| `feature/f41-ghcache-final-20260912`, `dccd071` | 18 ahead, 0 behind; clean | Shares native-TUI history, adds cached review-notification adapter; differs from final visible-TUI branch |
| `feature/tui-revive`, `123b3de` | 2 ahead, 0 behind; dirty | Dead coordinator revival plus uncommitted follow-up changes in control/job/mod/main and revival test |
| `fix/lane-list-liveness`, `9ca41fe` | 0 ahead, 160 behind; dirty | Uncommitted classifier tries live target, then live stored PID, then retired, else dead; adds JSON/header rendering |
| `fix/boop-lane-lifecycle-r2`, `796812e` | 0 ahead, 45 behind; dirty | Committed head is contained in main; lifecycle E2E test still modified |
| Instant `chore/f41-final-integration-20260912`, `e9241fae` | 22 ahead, 0 behind; clean | Recovered delivery branch with scoped Boop network work and test/cleanup fixes |
| Instant `fix/luna-scoped-network-20260912`, `e386b369` | 2 ahead, 0 behind; dirty | Additional uncommitted scoped-network files |
| Instant `fix/turn-attribution-input-traits`, `57e1b680` | 0 ahead, 170 behind; dirty | Backend and frontend attribution changes remain in worktree |

Native lane channel on `b52826f` keeps `session`, `baseline_seq`, `pending`, and `pending_event`. `start_turn` observes binding, captures baseline, submits, and sets pending. A matching native receipt clears pending and yields Done/Failed/Flaked; receipt requires the selected session and a pending turn. `next_event` handles frontend exit and adapter-idle/transcript receipt evidence. This state is channel-local; it is not an added process-state field in the screenshot table.

The lane-list patch still maps observation failure/no evidence to `dead` after its fallback. It therefore does not preserve the `Unknown` / `Inaccessible` distinctions from `RuntimeLiveness`.

## Formal data model draft for review

The following is a proposed contract derived from the separate axes above. It is not implemented by this audit.

```rust
struct ProcessKey { host: HostId, pid: u32, started_at: Timestamp }
struct PaneKey { server: TmuxServerId, pane: PaneId }
struct SessionKey { harness: HarnessId, native_id: String }
struct BindingKey { route: RouteId, generation: u64 }

enum ProcessState { Unknown, Running, Exited { code: Option<i32>, signal: Option<i32> } }
enum TurnState { Unknown, Idle, Running { turn: TurnId }, Waiting { turn: TurnId, reason: WaitReason } }
enum BindingState { Unresolved, Bound { session: SessionKey, process: ProcessKey }, Detached, Closed }
enum Residency { Unknown, RunningTurn, Parked, Retired }

struct Observation<T> { value: T, observed_at: Timestamp, source: SourceId }
struct SessionBinding {
    key: BindingKey,
    state: BindingState,
    pane: Option<PaneKey>,
    opened_at: Timestamp,
    closed_at: Option<Timestamp>,
}
struct LineageEvent {
    trace: TraceId,
    from: Option<SessionKey>,
    to: SessionKey,
    cause: LineageCause, // fresh, resume, clear, fork; compact may retain same session
    at: Timestamp,
}
fn reduce(state: Snapshot, event: ObservedEvent) -> Result<Snapshot, Conflict> {
    // Validate observer ownership and sequence for the binding generation.
    // Close/open binding intervals without rewriting earlier process lifetimes.
    // Apply explicit lineage event independently of process and turn state.
    // Update each observation axis; preserve unknown when a source fails.
}
```

| Lifetime / cardinality | Proposed rule |
|---|---|
| Process | Start-time-qualified identity; one process can host successive native sessions |
| Native session | Harness-qualified identity; may have multiple process bindings across resume |
| Route binding | Monotonic generation; stale events can only close/update their own generation |
| Trace | Durable grouping; explicit lineage events explain why sessions share it |
| Compact | Record a context event on the current session unless native evidence explicitly changes identity |
| Clear | Close old binding and open replacement binding; keep trace only through an explicit clear lineage event |
| Exit | Ends process/binding observation; retains native session and trace history |
| Resume | New process/binding generation points to an existing session |
| Fork | Explicit parent-child relation; trace grouping policy remains a separate decision |
| Unknown evidence | Retain source/error/freshness; failure to observe does not manufacture an exit |

Storage/read order: append observed event with producer sequence and binding generation; update process observation; close/open binding interval; record lineage if supplied; update turn/residency independently; publish one snapshot revision to consumers. Reads join exact route binding to session and process, then add trace membership and turn state. Existing runtime selection currently chooses the generated session with maximum activity timestamp, reporting ties as ambiguous. The proposed binding relation would make current selection explicit.

Suggested UI predicates are named by their actual measure: `process_running`, `turn_running`, `waiting_for_input`, `tmux_attached`, `route_reachable`, `recent_activity(since)`. A product-level “active” filter would specify which predicate it selects.

## Verification scope and retained inventory

Read source definitions and call sites, recovered the prior transcript, inspected Git worktree status and ancestry, and read lifecycle-test assertions. No new executable regression results are claimed.

Worktree counts from `git worktree list --porcelain`: hafley-rs 101 entries, 15 dirty existing checkouts, 90 entries with commits outside main; Instant 22 entries, 13 dirty, 15 with commits outside main. Entries include stale/prunable registrations and unrelated game work. Outside-main commits can include cherry-picked equivalents, shared ancestry between pending branches, or superseded work; these counts are not counts of missing fixes.

Full inventory: [worktrees JSON](2_boop-state-worktrees.json), [worktrees table](3_boop-state-worktrees.md). Stashes also exist: hafley-rs 2; Instant 5. Their contents were not restored or classified for integration.
