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
