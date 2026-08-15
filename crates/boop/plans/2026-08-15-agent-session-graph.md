# Agent session graph

## Goal

Expose the native session and parent-child relations already acquired by each
Boop harness adapter as one typed query. Instant's harness-trace and bottom
external-shell view consume this query without CASS terminology, CASS process
calls, raw Boop SQL, or direct transcript-store reads.

## Existing acquisition boundary

```rust
trait Harness {
    fn id(&self) -> &'static str;
    fn sessions(&self) -> anyhow::Result<Vec<SessionRef>>;
    fn read_from(&self, session: &SessionRef, offset: u64) -> anyhow::Result<ReadChunk>;
    fn ingest(&self, store: &Store, session: &SessionRef, from: u64) -> anyhow::Result<Ingested>;
}

struct SessionRef {
    harness: &'static str,
    session_id: String,
    parent: Option<String>,
    cwd: Option<String>,
    tmux: Option<String>,
}
```

`Claude`, `Codex`, `OpenCode`, and `Kimi` implement this trait. Sync writes
`SessionRef.parent` as an `agent_edge` with kind `spawned`. Preserve this
boundary. Provider-specific discovery and parsing remain inside each harness
implementation.

## Public relational projection

```rust
type LoadAgentSessionGraph =
    fn(&Store, AgentSessionGraphQuery) -> anyhow::Result<AgentSessionGraph>;

struct AgentSessionGraphQuery {
    cwd: Option<PathBuf>,
    include_history: bool,
}

struct AgentSessionGraph {
    schema_version: u32,
    sessions: Vec<AgentSessionNode>,
    edges: Vec<AgentSessionEdge>,
    shells: Vec<AgentShellNode>,
}

struct AgentSessionNode {
    session: String,
    harness: String,
    cwd: Option<PathBuf>,
    tmux: Option<String>,
    state: Option<String>,
    last_activity_ts: Option<u64>,
}

struct AgentSessionEdge {
    parent: String,
    child: String,
    kind: String,
}

struct AgentShellNode {
    lane: String,
    parent_lane: Option<String>,
    cwd: Option<PathBuf>,
    tmux: Option<String>,
    pid: Option<u32>,
    state: String,
}
```

The implementation may refine integer widths and existing newtypes. The JSON
field meanings and normalized relation boundaries remain stable.

## Instance timeline

1. Each harness discovers native sessions and provider-recorded parent ids.
2. Incremental sync projects sessions, turns, usage, live state, and spawned
   edges into Boop's store.
3. One runtime observation supplies tmux and process state for registered lanes,
   including shell-only lanes.
4. One set-wise store query returns session nodes and edges.
5. The public command returns one JSON document.
6. Instant builds the current terminal's native-descendant closure, subtracts
   that closure, and shows remaining live pane-owning rows as external shells.

## Storage and identity

- `agent_session` owns normalized harness sessions.
- `agent_edge(parent_session_id, child_session_id, edge_kind_id)` owns native
  parent-child relations.
- Existing trace tables continue to group resumed or replaced session ids.
- Lane/runtime tables own shell-only rows because a plain shell has no harness
  transcript session.
- Session identity remains provider-derived and harness-qualified where needed.
- A tmux pane is a runtime coordinate. It is not session identity.
- The graph projection performs no writes after incremental sync finishes.

## Audit receipt (2026-08-15)

### Provider acquisition fields

| Harness | Native session source | `SessionRef.session_id` | `SessionRef.parent` | `SessionRef.cwd` | activity source | tmux at discovery |
| --- | --- | --- | --- | --- | --- | --- |
| Claude | `harness/claude.rs::sessions_in` walks `~/.claude/projects/**/*.jsonl` | root: file stem; child: `<parent>/<file-stem>` | containing directory above `subagents/` | first complete JSONL record `cwd` | transcript file mtime | `None` |
| Codex | `harness/codex.rs::sessions_in` walks `~/.codex/sessions/**/*.jsonl` | `session_meta.payload.id` | `session_meta.payload.forked_from_id` | `session_meta.payload.cwd` | rollout file mtime | `None` |
| OpenCode | `harness/opencode.rs::Opencode::sessions` reads the `session` SQLite table | `session.id` | `session.parent_id` | `session.directory` | `session.time_updated` | `None` |
| Kimi | `harness/kimi.rs::sessions_in` walks `~/.kimi-code/sessions/*/session_*/agents/*/wire.jsonl` | main: session UUID; child: `<session UUID>/<agent directory>` | main session UUID for every non-`main` agent directory | `state.json.workDir` | wire file mtime | `None` |

`SessionRef` also has `nickname`, transcript/store `path`, `git_branch`,
`modified_ms`, `size`, `tmux_socket`, and `tmux`. All discovery paths above
leave the two tmux fields unset. Spawn methods manufacture a provisional
`SessionRef`; Codex documents that its returned id is not the later rollout
id, OpenCode does the same for its database-generated id, and Kimi uses the
lane name rather than the later native session UUID. Spawn-returned ids cannot
be treated as graph-session identities without a subsequent native match.

### Corrected public-field contract

| Proposed field | Corrected contract | Existing source |
| --- | --- | --- |
| `AgentSessionNode.session` | Canonical graph key. It must be harness-qualified or represented by the `(harness, native_session_id)` tuple. Both every edge endpoint and every route `sessionId` join use this key. A bare provider id is insufficient because Boop's `dict_session` and `agent_session.session_id` currently key only a string. | `harness.rs::SessionRef`; `ident.rs::Store::session_id`; `bus.rs::Route.session_id` |
| `AgentSessionNode.harness` | Stable adapter id: `claude`, `codex`, `opencode`, or `kimi`. It is metadata on `agent_session`, not part of the current database primary key. | `ident.rs::upsert_session_row`; `agent_session` schema |
| `AgentSessionNode.cwd` | Optional, raw provider workspace path. It is not display tildification, canonicalization, or a tmux identity. `--cwd` filtering must state its path comparison rule before implementation. | the four acquisition paths above; Instant `harness.rs::tildify` |
| `AgentSessionNode.last_activity_ts` | Provider activity time in Unix milliseconds: transcript/wire mtime for Claude, Codex, Kimi; `time_updated` for OpenCode. It is distinct from sync-observation time. | `SessionRef.modified_ms`; Instant `0a_harness_trace_index.rs` |
| `AgentSessionNode.state` | Requires a defined runtime observation rule. `sync_session_with_pid` currently writes `live` only when `SessionRef.tmux.is_some()`, otherwise `idle`; discovery provides `None`. Instant instead derives `dead` from missing cwd and `live`/`idle`/`done` from activity age, then changes routed rows to `done` when their tmux route vanishes. | `ident.rs::sync_session_with_pid`; Instant `harness.rs::trace_status`; `0_mail.ts::settleRoutedStatus` |
| `AgentSessionNode.tmux` | Split this into a route/session target and a pane id if both are required. `SessionRef.tmux` is a spawn/control target, while `agent_live.tmux_pane_id` is named as a pane column but is currently populated from that target. Instant consumes a tmux session name. | `harness.rs::SessionRef`; `ident.rs::record_status`; `summary.rs::AgentSummaryAgent`; Instant `2_join.ts` |
| `AgentSessionEdge.parent` / `child` | Canonical graph keys, using the same identity rule as `session`. `kind` is `spawned` for provider-recorded parent relations. | `ident.rs::add_edge`; `agent_edge` schema |
| `AgentShellNode.lane` | Route/lane identity, separate from a native harness session identity. A shell-only row is emitted from a route with `harness: None` or `kind: "shell"`; it must never mint an `agent_session` merely to obtain an id. | `bus.rs::Route`; `ident.rs::LaneSpawn`; `summary.rs` shell fixture |
| `AgentShellNode.parent_lane`, `cwd`, `tmux`, `pid`, `state` | Runtime route fields: `Route.parent`, `Route.cwd`, `Route.tmux`, pane-derived pid, and a declared liveness rule. `agent_lane` persists spawn intent but has no tmux, pid, mode, or current-state columns. | `bus.rs::Route`; `ident.rs::agent_lane`; `runtime.rs` through `summary.rs` |

### Existing projection path

`main.rs::sync_all` and `run_follow` enumerate every adapter's
`Harness::sessions()`. `ident.rs::sync_session_with_pid` obtains the cursor,
calls adapter `ingest`, records `agent_live`, conditionally upserts
`agent_session`, conditionally inserts a `spawned` edge, then advances the
cursor. `agent_edge` joins `dict_session` endpoints and `dict_edekind` for
`Store::query_edges` / `Store::edge_rows`; `agent_live` and
`agent_live_span` carry status, pid, and the currently misnamed pane/target
value. `Store::backfill_traces` groups existing `spawned` edges only after
they have been written.

The `agent_session` upsert and `agent_edge` insertion are guarded by
`ingested.stat.written > 0 || ingested.stat.usage_written > 0`. A discovered
session whose first sync yields no projected turn or usage has a
`dict_session`/`agent_live` entry but no `agent_session` row or native edge.
`sync_all` also bypasses `sync_session_with_pid` when file byte length equals
the stored cursor. The graph query must define whether its universe is all
currently discovered sessions or only successfully projected transcript rows,
then make session-row and edge projection conform to that decision.

### Instant consumer receipt

`src-tauri/src/harness.rs::harness_trace_rows` reads
`HarnessStore::trace_sessions`, whose active implementations delegate to
`src-tauri/src/0a_harness_trace_index.rs`, not the broader per-cwd readers in
`0_harness_store.rs`.

- Claude: trace index preserves filesystem `subagents` parent directories.
- Codex: trace index reads `~/.codex/state_5.sqlite`, `threads`, and
  `thread_spawn_edges.parent_thread_id`; this differs from Boop's rollout
  `session_meta.payload.forked_from_id` source.
- OpenCode: trace index reads active `session` rows but emits no `parent_id`,
  even though Boop reads `session.parent_id`.
- Kimi: trace index emits only each `agents/main/wire.jsonl`; Boop emits
  every agent directory and parent relation. Kimi subagents therefore have no
  Instant trace seed today.

`HarnessTraceRow` is converted to `AgentSessionNode` by
`plugins/harnessTrace/0_tree.ts::toAgentNodes`. `parentId` / `parentKind`
feed native closure. The mail ledger may add a `dispatch` parent only to a
node that has no provider parent relation.

`0_strip.ts::nativeSessionIds` selects either the explicit
`nativeSessionId` or root node(s) joined to the current tmux session, then
adds only transitive `parentKind === "subagent"` children. `inScope` computes
the descendant closure from that native set over all nodes. `external` then
keeps `status === "live"`, excludes subagent rows, and requires a non-null
`tmuxSession`; `history` retains all statuses and subagent rows after the
same scope and native subtraction.

`DockStripShared.tsx::attachTmux` takes route-pinned tmux first, then
`2_join.ts::assignTmuxPanes` assigns at most one going non-subagent claimant
per tmux row. Its inputs are node harness, untildified cwd, last activity,
route tmux, and `live && non-subagent`; it accepts a matching harness process
or a plain shell process. `joinTmuxSessions` retains every cwd/chip-path
match for related-scope selection. Route disappearance changes a routed node
to `done` in `0_mail.ts::settleRoutedStatus`.

`HarnessTracePanel.tsx` additionally invokes
`cass_swarm_status`; `src-tauri/src/ledger.rs` executes
`cass swarm status --robot-format json` in the selected cwd. This call feeds
the panel's CASS status line and has no equivalent field in the proposed
agent-session graph.

### Open contract findings

1. Decide and document the canonical session key before querying or joining.
   The present Boop store, routes, and Instant tree all use bare strings;
   Claude and Kimi child ids include their parent but root ids and Codex /
   OpenCode ids remain unqualified.
2. Decide the authoritative Codex parent source. Boop reads rollout metadata;
   Instant's trace reader reads `thread_spawn_edges`. The audit does not prove
   their equivalence.
3. Decide whether OpenCode `session.parent_id` and Kimi non-main agent
   directories are in the initial public relation. Boop discovers both;
   Instant's current trace index does not expose either relation.
4. Define a runtime observation that yields tmux target, pane id, pid, and
   state together. `SessionRef.tmux` and `agent_live.tmux_pane_id` have
   incompatible current meanings; transcript discovery does not populate
   either.
5. Define current-versus-history selection. `agent_session` and
   `agent_edge` are append/projected facts, routes are current registry data,
   and the proposed `include_history` does not say which source governs a
   vanished transcript, lane, or process.
6. Retain the existing Instant shell-mode inputs in the migration boundary:
   registry `Route.kind`, `harness`, `tmux`, `cwd`, `mode`, `session_id`, and
   `parent`. The proposed `AgentShellNode` omits `mode`, `harness`, and the
   route session join.

### Focused audit tests

Boop command run from the Boop worktree:

```text
cargo test -p boop harness
```

It covers the Claude duplicate-stem and subagent-parent fixture, Codex
forked-session fixture, and Kimi main/subagent fixture. OpenCode has a
missing-store test but no fixture proving `session.parent_id` projection.

Instant commands to run from `/Users/chrishafley/projects/instant` for the
covered pure fixture surfaces:

```text
pnpm vitest run src/plugins/harnessTrace/0_strip.test.ts src/plugins/harnessTrace/2_join.test.ts src/plugins/harnessTrace/0_mail.test.ts
cargo test --manifest-path src-tauri/Cargo.toml harness::tests
```

The first command covers native-descendant subtraction, related/all scope,
subagent suppression, shell-process pane assignment, shared-cwd tmux matches,
registry-only shell rows, and routed status settling. The Rust target covers
the trace-index Claude and Codex parent fixtures; it does not currently prove
OpenCode parent links or Kimi non-main agent rows.

## Projection correction decisions

- `include_history = false` keeps every discovered native session except rows
  explicitly observed as `dead`; discovery without a tmux target is `idle`,
  and remains current native evidence. Shell rows require current route/lane
  evidence and a live pane; `--history` includes dead shell rows.
- Shell identity comes from the route registry (`kind == "shell"` or no
  harness). Durable lane rows with a harness remain route-backed native rows;
  the public shell node retains route harness, mode, and session-join fields.
- Public native identities are `{harness, id}` pairs and every edge endpoint
  uses the same type. The existing `dict_session` unique bare-string key can
  already have merged cross-harness collisions, so this release reports the
  stored harness and defers a storage-key migration that would recover lost
  identity.
- The public edge relation requires both endpoint sessions in the filtered
  graph. Provider-discovered parent ids whose parent transcript is absent stay
  in durable `agent_edge` and are omitted from the public projection.
- Discovery metadata and parent edges are projected before the unchanged-byte
  gate. Empty or unchanged transcripts therefore still enter the graph without
  reparsing or runtime acquisition.

## CLI

```text
boop agent sessions [--cwd <path>] [--history] --format json
```

The initial public contract requires JSON. Text output may be added only when a
human-facing use case defines its columns. Public help uses `agent sessions`,
`session graph`, and `external shells`. It contains no swarm vocabulary.

## Instant migration

Replace these acquisition paths while preserving rendering and strip policy:

- `cass_swarm_status`
- direct Claude, Codex, OpenCode, and Kimi session enumeration for covered
  graph fields
- ad hoc joins that Boop's graph JSON already supplies

Keep Instant's native-descendant subtraction, related/all scope, pane ownership,
history toggle, placement, and rendering in Instant.

## Tasks

1. Audit the exact parent, identity, cwd, status, and tmux semantics emitted by
   all four harness adapters and the Instant bottom-view consumers.
2. Add the typed set-wise graph query and deterministic fixtures in Boop.
3. Add `boop agent sessions --format json`, incremental sync, schema version,
   and help.
4. Replace Instant's CASS and direct-ledger acquisition for the covered fields.
5. Run fixture parity for native subagents, nested subagents, external lanes,
   plain shell lanes, dead sessions, shared cwd, and one-pane ownership.

## Gates

```text
cargo test -p boop
cargo clippy -p boop --all-targets -- -D warnings
pnpm vitest run src/plugins/harnessTrace src/boopAgents.test.ts
cargo test --manifest-path src-tauri/Cargo.toml harness_store
issuectl doctor
```

The Instant commands are run from the Instant repository. Existing unrelated
gate failures must be recorded with their exact test names and output.

## Non-goals

- Provider issue, reservation, or project-management data
- UI layout changes
- Deleting Instant's old acquisition code before fixture parity
- Replacing Boop's SQLite storage schema solely to expose this projection
- A public command or type containing `swarm`
