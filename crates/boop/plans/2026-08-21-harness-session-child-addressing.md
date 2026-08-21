# Harness session and child addressing

## Goal

Give Boop one durable address model for interactive harness sessions and their
children. Messages must use harness-native control or the lane supervisor.
Terminal panes remain process-location evidence and never carry agent messages.

This closes the failures observed on 2026-08-21:

- `codex-3180` retained a pane route after resume while `session_id` and the
  Codex app-server socket were absent, so a Claude result was queued.
- Claude peer UDS reached the exact parent session.
- A Codex Luna child accepted a direct native collaboration follow-up.
- A Claude Haiku subagent accepted a message through its parent and returned
  the nonce to that parent.
- A stale project hook consumed mail addressed to a different Claude session.

## Boundaries

- Harness implementations own discovery and control mechanics.
- Mail routing calls the `Harness` trait and contains no Codex, Claude,
  OpenCode, Kimi, ACP, socket, command, or transcript-specific branch.
- Multiplexer state locates a process. It is never a message transport.
- Ordinary Claude subagents are parent-mediated because Claude exposes their
  return channel through the parent session.
- Direct child delivery is advertised only after an executable test proves it.
- Registry readers remain compatible with existing registry JSON.
- No tmux `send-keys`, PTY writes, prompt hooks, or input-buffer injection may
  deliver Boop mail.
- No automatic agent-team enablement and no change to provider permissions.

## Type signatures

The names below describe required responsibilities. Existing crate placement
may adjust when dependency direction requires it.

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HarnessSessionId {
    pub harness: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ControlEndpoint {
    CodexRemote { socket: PathBuf },
    ClaudePeer { socket: PathBuf, pid: u32, proc_start: String },
    AcpSession { agent: String, session_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessSession {
    pub id: HarnessSessionId,
    pub cwd: Option<PathBuf>,
    pub control: Option<ControlEndpoint>,
    pub observed_process: Option<u32>,
    pub observed_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChildAddress {
    pub parent: HarnessSessionId,
    pub child_id: String,
    pub kind: ChildKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChildKind {
    Direct,
    ParentMediated,
    ObservableOnly,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentAddress {
    Session(HarnessSessionId),
    Child(ChildAddress),
    Lane { name: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeliveryReceipt {
    Accepted,
    Delivered,
    ParentMediated { parent: HarnessSessionId },
    Queued { reason: QueueReason },
    Unsupported { capability: &'static str },
}

pub trait Harness: Send + Sync {
    fn discover_session(
        &self,
        observation: &ProcessObservation,
    ) -> anyhow::Result<Option<HarnessSession>>;

    fn refresh_session(
        &self,
        known: &HarnessSession,
        observation: Option<&ProcessObservation>,
    ) -> anyhow::Result<Option<HarnessSession>>;

    fn send_session(
        &self,
        session: &HarnessSession,
        message: &str,
    ) -> anyhow::Result<DeliveryReceipt>;

    fn send_child(
        &self,
        child: &ChildAddress,
        message: &str,
    ) -> anyhow::Result<DeliveryReceipt>;
}
```

`ControlEndpoint` is typed. Do not store a generic URL or untyped JSON object.
Provider secrets remain in provider-owned files or environment. Persist only
the locator required to reconnect.

`ProcessObservation` contains the process/pane evidence already available
through `Multiplexer` and `ProcReader`:

```rust
pub struct ProcessObservation {
    pub pane: Option<String>,
    pub pane_pid: Option<u32>,
    pub descendants: Vec<ProcessInfo>,
    pub cwd: Option<PathBuf>,
}
```

The implementation may borrow existing process types instead of copying them.

## Instance lifetimes

### Route

A route is a human or coordinator name such as `codex-3180`. It survives
temporary process metadata loss. It points to an `AgentAddress` and retains
mailbox, parent, goal, worktree, model, and registration facts.

### Harness session

A harness session represents the provider conversation. Its identity is
`(harness, provider_session_id)`. It can survive terminal replacement, process
restart, pane movement, or worktree reuse. Control endpoint observations may
expire and refresh without changing session identity.

### Control endpoint

A control endpoint belongs to one live harness process or ACP session. It is
replaceable state. A socket disappearing marks the endpoint stale; it does not
delete the provider session or route.

### Child address

A child belongs to one parent harness session. The same textual child ID under
two parents names two children. Completion may make the child non-live while
its transcript and parent edge remain queryable.

### Pane

A pane locates a process tree and supports display, focus, lifecycle, and
resource observation. Moving or closing a pane must not alter conversation
identity. No delivery method accepts a pane identifier.

## Storage and uniqueness

Add a session registry beside the route registry, using an atomically replaced
JSON file initially. Do not introduce SQLite for this slice.

```text
registry.json          existing route names and route metadata
sessions.json          harness session identity and current control observation
bus.ndjson             messages and acknowledgements
```

Required keys:

```text
route key:             route_name
harness session key:   harness + provider_session_id
child key:             harness + parent_session_id + child_id
control observation:   harness session key, at most one current endpoint
```

Required route fields after migration:

```rust
pub struct Route {
    // existing route facts remain
    pub address: Option<AgentAddress>,

    // compatibility fields read from old rows, omitted on newly written rows
    pub session_id: Option<String>,
    pub app_server_socket: Option<String>,
}
```

Writing a route and its session observation must use one store operation with
temporary files and rename. If the current registry abstraction cannot commit
both files atomically, serialize writers with the existing registry lock and
write the session file before the route that references it. A missing session
row leaves mail queued.

Never persist Claude peer tokens. Resolve the token from the exact live
descriptor/key pair at send time and require matching PID plus `procStart`.

## Read and write sequence

### Native TUI registration

```text
boop tui <harness>
  -> Harness::prepare_native_tui
  -> launch or attach provider process
  -> observe provider session ID and control endpoint
  -> upsert HarnessSession
  -> write Route.address = Session(session ID)
```

Registration must happen again after resume. A resumed TUI may reuse the
provider session identity and replace its control endpoint.

### Sending to a top-level session

```text
hail(route_name, body)
  -> append durable message
  -> resolve Route.address
  -> load HarnessSession
  -> validate current endpoint
  -> refresh through Harness when absent or stale
  -> Harness::send_session
  -> append delivery acknowledgement only for Accepted or Delivered
```

`ParentMediated` is also accepted delivery, but the receipt records the parent
session used. `Queued` and `Unsupported` do not acknowledge the message.

### Sending to a child

```text
hail(child_route, body)
  -> resolve ChildAddress
  -> load parent HarnessSession
  -> ChildKind::Direct
       -> Harness::send_child
     ChildKind::ParentMediated
       -> Harness::send_child, implementation prompts the parent control API
          to resume/message the exact child
     ChildKind::ObservableOnly
       -> queue with typed unsupported receipt
```

The mail layer does not construct prose asking a parent to relay a message.
That provider-specific instruction or API call belongs inside the harness.

### Return messages

Every dispatched message carries the Boop message ID in harness metadata or
the provider prompt envelope. A child result is appended as a Boop message with
`reply_to` equal to the originating ID. Matching by body text is prohibited.

## Harness behavior

### Codex

- Discover the provider thread ID from the process command, transcript, or
  app-server session inventory.
- Discover the current managed app-server socket independently from a route.
- `send_session` invokes the supported remote queue command for the exact
  thread and socket.
- `send_child` uses the native collaboration child handle when exposed to the
  harness adapter. Do not approximate this with a second top-level thread.
- Preserve the model override used at child creation, including
  `gpt-5.6-luna`.
- If the installed Codex control surface cannot enumerate collaboration child
  handles outside the parent runtime, advertise `ParentMediated` and route
  through the parent thread until a direct API is measured.

### Claude

- Discover live peer descriptors under `~/.claude/sessions` by exact session
  ID, live socket, newest `updatedAt`, matching PID, and matching `procStart`.
- Resolve the authentication key at send time. Never persist `peerToken`.
- `send_session` sends authenticated newline-delimited peer frames.
- Ordinary Agent-tool subagents use `ParentMediated` because they report to the
  parent session.
- When `SendMessage` is available and the child has an agent ID, resume or
  message that exact child through Claude's native mechanism.
- Agent-team teammates may be `Direct` only when the experimental feature is
  already enabled by the user and the teammate name/ID is observed.
- Support `model: haiku` as child metadata; Boop must not silently change the
  parent's model.

### ACP lanes

- The lane supervisor owns one ACP connection and one ACP session ID.
- Lane mail remains queued in the mailbox until the supervisor begins the next
  `session/prompt` turn.
- Do not manufacture a second ACP connection from a stored session ID.
- ACP child visibility depends on session updates from the external agent.

### OpenCode and Kimi

- Implement discovery first.
- Return typed `Unsupported` for session or child sends until their native API
  is proven by an integration test.
- No PTY fallback.

## Capability representation

Replace the single ambiguous control boolean with addressable capabilities:

```rust
pub struct ControlCapabilities {
    pub session_send: bool,
    pub child_send: ChildSendCapability,
    pub endpoint_refresh: bool,
}

pub enum ChildSendCapability {
    Direct,
    ParentMediated,
    ObservableOnly,
}
```

CLI output must report these fields without inferring them from transcript
visibility.

## Implementation DAG

### 1. Store types and compatibility

Files:

- `crates/boop-store/src/bus.rs`
- a numerically ordered session-address module under `crates/boop-store/src/`
- matching store tests

Work:

- Add typed identifiers, addresses, endpoints, receipts, and session store.
- Read old route fields into a synthesized session address when possible.
- Keep old registry fixtures readable.
- Write new address fields deterministically.
- Prove uniqueness and atomic replacement behavior.

Acceptance:

- Two checkouts in one cwd can have distinct sessions.
- One provider session can move panes without changing identity.
- Two children with the same child ID under different parents do not collide.
- A stale endpoint refresh changes only endpoint observation fields.

### 2. Harness trait and discovery seam

Files:

- `crates/boop-harness/src/harness.rs`
- `crates/boop-harness/src/lib.rs`
- harness-specific modules

Work:

- Introduce `discover_session`, `refresh_session`, `send_session`, and
  `send_child`.
- Remove `NativeSessionRef` after all callers use `HarnessSession`.
- Keep provider mechanics inside each harness implementation.
- Add pure fixture-based discovery tests.

Acceptance:

- Mail code contains no provider name checks.
- Mail code imports no Unix socket or process-command type.
- Harness implementations contain all provider-specific endpoint logic.

### 3. Codex durable return path

Files:

- `crates/boop-harness/src/harness/codex.rs`
- native TUI registration in `crates/boop/src/cli/control.rs`
- route/session reconciliation in `crates/boop/src/cli/me.rs`

Work:

- Refresh a missing Codex thread ID from pane/process evidence.
- Refresh a missing or dead remote socket through supported Codex control.
- Persist the refreshed session independently from the route.
- Deliver by exact thread ID.

Acceptance:

- Start through Boop, resume, replace endpoint, then receive a reply.
- A route with correct pane and absent legacy socket succeeds after refresh.
- A route naming another Codex thread receives no message.

### 4. Claude durable session path

Files:

- `crates/boop-harness/src/harness/claude.rs`

Work:

- Factor descriptor/key selection into pure functions over supplied paths.
- Test duplicate descriptors, stale sockets, PID reuse, mismatched
  `procStart`, zero keys, and multiple keys.
- Return a receipt only after the peer frame is accepted by the live socket.

Acceptance:

- Exact session transcript records one peer-origin message.
- A second live Claude session records zero copies.
- No hook installation is involved.

### 5. Child address projection

Files:

- harness transcript projection modules
- session graph modules in `boop-store`
- agent registration CLI

Work:

- Project parent session ID, child ID, child kind, model, and completion state.
- Register child routes with `AgentAddress::Child`.
- Keep historical completed children queryable.

Acceptance:

- Claude Haiku child appears under its exact parent and model.
- Codex Luna child appears under its exact parent and model.
- A native child never claims the parent's pane.

### 6. Child delivery

Files:

- `crates/boop-harness/src/harness/codex.rs`
- `crates/boop-harness/src/harness/claude.rs`
- `crates/boop/src/cli/mail.rs`

Work:

- Implement measured direct or parent-mediated provider paths.
- Carry originating Boop ID through the dispatch envelope.
- Append typed delivery receipts.

Acceptance:

- Codex parent sends nonce to Luna child and receives exact nonce with
  `reply_to` preserved.
- Claude parent sends nonce to Haiku child and receives exact nonce with
  `reply_to` preserved.
- Sending to a completed or unaddressable child stays queued with a named
  reason.

### 7. Remove compatibility delivery

Files:

- `crates/boop/src/cli/mail.rs`
- hook/inbox commands and tests
- docs and help text

Work:

- Remove hook inbox as an agent-message transport.
- Keep mailbox durability and wait semantics.
- Remove help text that says hail types into a pane.
- Retain tmux only for lifecycle, focus, capture, layout, and observation.

Acceptance:

- `rg 'send_keys_literal|pane.send_input|WriteUserInput'` finds no Boop mail
  delivery call.
- A partially typed prompt remains byte-identical during every E2E test.

## CI

New CI coverage required:

```text
boop-store
  legacy route compatibility
  typed session/address round trip
  uniqueness and endpoint replacement

boop-harness
  Codex discovery and endpoint refresh fixtures
  Claude descriptor/key/socket selection fixtures
  control capability matrix

boop CLI
  route -> session -> native delivery
  missing endpoint refresh
  queued unsupported child
  no PTY fallback

live opt-in receipts
  Codex parent -> Luna -> parent
  Codex parent -> Claude parent -> Haiku -> Claude parent -> Codex parent
```

The live tests must be opt-in because they consume provider tokens. They write
machine-readable receipts containing session IDs, child IDs, models, message
IDs, elapsed time, and delivery path. Unit and integration CI use fake control
endpoints and deterministic fixtures.

## Documentation changes

- `boop --help`: replace every statement about typing or injecting keys with
  native session, supervisor, or queued delivery semantics.
- `crates/boop/docs/tell.md`: show address resolution and receipt flow.
- Harness capability output: report session send, child send mode, and endpoint
  refresh separately.
- Document that Claude ordinary subagents require parent mediation and Claude
  agent-team teammates can be directly named when already enabled.
- Document that model choice belongs to child creation: Luna for Codex and
  Haiku for Claude are preserved in projected metadata.

## Refusals and queue reasons

Every non-delivery must name one of these conditions:

```rust
pub enum QueueReason {
    RouteMissing,
    AddressMissing,
    SessionMissing,
    EndpointMissing,
    EndpointStale,
    ChildCompleted,
    ChildObservableOnly,
    HarnessUnsupported,
}
```

Provider command failure, authentication failure, malformed descriptors, and
ambiguous exact matches are errors with evidence. They are not converted into
`Unsupported` or acknowledged.

## Completion definition

The arc is complete when:

1. Routes reference durable harness sessions instead of owning control
   endpoints.
2. Codex and Claude top-level native delivery survive resume and endpoint
   replacement.
3. Luna and Haiku child paths pass the nonce E2E tests with model and parent
   identity recorded.
4. Boop mail delivery contains no tmux, PTY, prompt-hook, or input-buffer path.
5. Existing route files remain readable.
6. New CI builds and tests the store, harness, and CLI paths.
