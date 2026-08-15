# Instant agent projection from Boop

## Scope

Boop supplies one typed agent-activity projection. Instant remains unchanged during this phase. SQLite stores normalized transcript and runtime facts; the Rust query layer derives panel rows without exposing dictionary IDs or table names.

## Current Instant inputs

| Consumer | Current input | Current derivation |
|---|---|---|
| `src/boopAgents.ts` | `beep lane list`, `beep ps`, `beep pstree`, `db session list`, `db usage` | Joins by lane and harness; polls runtime every tick and sessions/usage every fifth tick |
| `src-tauri/src/harness.rs` | Direct Claude, Codex, OpenCode, and Kimi stores | One session row per discovered harness session |
| `src-tauri/src/ledger.rs` | Direct harness transcripts | User, assistant, reasoning, tool-call, and tool-result message rows |
| `src/plugins/harnessTrace/0_mail.ts` | Mail registry and bus NDJSON | Route/session fallback, parent sender, and dispatch reason |
| `src/plugins/harnessTrace/2_join.ts` | Tmux pane cwd and foreground process | Session-to-pane and worktree matching |
| `src/plugins/cass/` | `cass swarm status` | Provider, issue, reservation, agent, message, and call presentation |

## Boop facts already available

| Fact | Owner |
|---|---|
| normalized user, assistant, and tool turns | `agent_turn` |
| token buckets and request calls | `agent_usage` |
| trace and attached sessions | `agent_trace`, `agent_trace_span` |
| lane spawn identity | `agent_lane` |
| parent and dispatch edges | `agent_edge` |
| current and historical process state | `agent_live`, `agent_live_span` |
| route registry and mailbox fold | `bus.rs` |
| tmux and process observation | `boop-mux`, `proc.rs` |
| lane-to-current-session resolution | `runtime.rs` |

## Counting contract

```rust
pub struct AgentActivityCount {
    pub user: u64,
    pub assistant: u64,
    pub tool_call: u64,
    pub total: u64,
}

pub fn agent_activity(query: AgentActivityQuery) -> Result<Vec<AgentActivity>, BoopError>;
```

- Session counts group `agent_turn` by session and role.
- Trace counts sum sessions attached through `agent_trace_span`.
- Lane counts use the trace selected by `runtime::resolve`.
- Tool results require an explicit normalized fact before they can be counted separately. Current `agent_turn` stores tool calls and omits tool results.
- Usage calls count `agent_usage` rows. Token and cost totals retain the existing five token buckets and price join.
- Mailbox inbox, outbox, and unacknowledged counts derive from the folded bus rows.
- Shell-only routes remain visible with zero transcript counts and nullable session identity.

## Acquisition, storage, derivation, presentation

| Layer | Owner |
|---|---|
| harness transcript parsing and sync cursors | Boop |
| route, mailbox, tmux, pane, and process acquisition | Boop |
| normalized transcript/runtime facts | Boop SQLite |
| session, trace, and lane aggregation | Boop typed Rust query |
| stable JSON command | Boop CLI |
| panel tree, expansion, selection, and rendering | Instant |
| CASS issue and reservation display | unchanged until a separate product decision |

## Gaps to close on the Boop side

1. Add the typed activity projection and stable JSON command.
2. Include shell-only routes with zero transcript counts.
3. Return route, tmux, pane, PID, process totals, parent, completion, and worktree coordinates in the same bounded observation.
4. Define tool-result retention or explicitly mark it unavailable.
5. Project folded mailbox counts without exposing NDJSON paths.
6. Keep CASS issue, reservation, and provider records separate from agent transcript/runtime counts.
7. Add a CASS-compatible agent summary from Boop facts: active agents, messages, calls, completion, and liveness.

## Required fixtures

- Claude root, subagent, resume, and compaction.
- Codex replacement session and tool call.
- OpenCode generated session and dispatch parent.
- Kimi child agent and usage update.
- Shell-only registered route with no transcript.
- Dead tmux route and historical live span.
- Equal-activity ambiguity.
- Mailbox completion and unacknowledged message.
- CASS-compatible summary with agent messages/calls kept separate from issue and reservation rows.

## Sequence

```text
runtime identity
      |
      +--> transcript and mailbox activity projection
      |
      +--> bounded tmux/process projection
                    |
                    v
          CASS-compatible Boop summary
```
