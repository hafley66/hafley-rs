# Derived trace identity from durable observations

## Scope

Trace membership is a projection over session identity observations. Writers record observations with provenance and timestamps; transcript sync and trace queries derive connected components. The current `agent_trace_span` remains a compatibility projection while readers migrate. The existing `agent_trace_event` remains the event log for supervisor/channel events and is not used as a substitute for identity observations.

## Type signatures

```rust
pub struct SessionObservation {
    pub observation_key: String,
    pub session_id: String,
    pub observed_at_ms: u64,
    pub harness: Option<String>,
    pub cwd: Option<String>,
    pub pid: Option<u32>,
    pub parent_pid: Option<u32>,
    pub pane_id: Option<String>,
    pub tui_session_id: Option<String>,
    pub source: String,
}

pub struct SessionRelation {
    pub relation_key: String,
    pub from_session: String,
    pub to_session: String,
    pub kind: SessionRelationKind,
    pub source: String,
    pub observed_at_ms: u64,
}

pub enum SessionRelationKind {
    ContinuedIn,
    SameProcess,
    ParentChild,
    SamePane,
    SameTui,
}

pub struct TraceJoinLimits {
    pub max_observations: usize,
    pub max_relations: usize,
    pub max_sessions: usize,
}

pub struct DerivedTrace {
    pub sessions: Vec<String>,
    pub root_session: String,
    pub evidence: Vec<SessionRelation>,
}

pub fn derive_traces(
    observations: &[SessionObservation],
    relations: &[SessionRelation],
    limits: TraceJoinLimits,
) -> Result<Vec<DerivedTrace>, TraceJoinDiagnostic>;
```

The schema keeps raw identifiers as nullable, source-scoped values. A normalized identity key includes the field kind and source namespace, so a pid and a pane string never compare equal. `observation_key` and `relation_key` are producer-stable idempotency keys.

## Join rules

1. Explicit `continued-in` transcript relations connect the named old and new session IDs. They are exact edges and do not depend on time proximity.
2. Matching `(harness, tui_session_id)` connects observations only when the harness defines that identifier as stable across session rotation.
3. Matching `(harness, host incarnation, pid)` is a candidate relation bounded by a configured time gap. Parent pid is evidence for a parent-child relation, not sufficient evidence of session continuation by itself.
4. Pane IDs require a tmux server incarnation. A pane string without that incarnation is diagnostic evidence only because `%0` can be reused after server restart.
5. Cwd and wall-clock proximity rank candidate evidence but never create an edge on their own.
6. Conflicting strong edges produce an ambiguity diagnostic and do not merge components automatically.
7. Component membership is derived. `agent_trace_span` materialization records the chosen component and provenance, and can be rebuilt from observations and relations.

Each edge retains its source, observed time, and matching key. Trace names are assigned from the earliest member session for stable output; no producer process asserts the final trace name.

## Instance timeline

1. A supervisor, harness observer, or transcript adapter emits an observation when it sees a session or a relevant identity change.
2. The writer inserts the observation and any exact relation in one SQLite transaction using producer-stable keys.
3. Transcript sync records a `continued-in` relation when a source transcript contains it, including when the old writer process has exited.
4. A trace query loads observations and relations for the requested candidate scope, then derives connected components under explicit limits.
5. The query returns component membership and edge evidence; it does not modify raw observations.
6. A separate idempotent materializer updates `agent_trace` and `agent_trace_span` for existing readers.
7. Re-running ingestion or materialization produces the same relations and spans.

## Storage and reads/writes

Add `agent_session_observation` with a unique `observation_key`, `session_id`, `observed_ts`, nullable identity columns, and `source_id`. Add `agent_session_relation` with a unique `relation_key`, endpoint session IDs, `kind_id`, nullable matched identity key, `source_id`, and `observed_ts`. Index endpoint session IDs and `(source_id, observed_ts)`. Store identity values in source-scoped dictionaries or a typed identity table; do not use a single untyped dictionary for pids, pane IDs, and TUI IDs.

Writers append observations and relations; they do not update a trace membership row. Ingestion uses `INSERT OR IGNORE` for stable producer keys. The reader selects observations for candidate sessions, loads incident relations under the limits, derives components, then writes the compatibility projection in one transaction. Uniqueness is producer key plus source namespace. Candidate limits stop traversal and return a named diagnostic rather than a partial successful trace.

## Bounded join

```text
queue := requested sessions
seen := empty set
edges := empty list
while queue is not empty:
    if seen.len >= max_sessions or edges.len >= max_relations:
        return TraceJoinDiagnostic::BudgetExceeded { limit, observed }
    session := queue.pop_front()
    if seen.insert(session):
        observations := load(session, max_observations)
        exact := explicit relations incident to session
        candidates := strong scoped identity matches(observations)
        reject ambiguous conflicts; retain diagnostics
        edges.extend(exact + candidates)
        queue.push_back(new endpoints)
return connected components(seen, edges)
```

The diagnostic names the exceeded limit and the root session. It includes counts, not identity values, to avoid leaking process or path data into routine logs.

## Lifetimes and compatibility

Observation rows outlive supervisor, wrapper, tmux server, and machine restart. Process and pane identity keys include the host or server incarnation needed to prevent reuse collisions. Transcript relations survive independently of the process that created the transcript. Materialized spans are replaceable derived state. During rollout, existing `attach_trace` rows seed observations once with source `legacy-agent-trace-span`; live writers then move to observation writes. Existing trace readers continue using spans until each read path switches to derived results.

## Verification gates

- Fixture: six session IDs across clear, compaction, resume, wrapper death, and machine restart derive one trace from stored observations and relations.
- Fixture: unrelated sessions sharing cwd and near timestamps remain separate.
- Fixture: reused pane `%0` under different server incarnations remains separate.
- Fixture: exact `continued-in` relation reconnects after restart and is idempotent on repeated sync.
- Fixture: a conflicting exact relation returns ambiguity and leaves components unmerged.
- Fixture: each configured limit returns its named diagnostic at the exact boundary.
- Migration receipt: existing `agent_trace_span` data seeds observations without changing query membership.
- Measurement: `BOOP_NATIVE_PROJECT_EVERY_MS` observation write rate, daily row count, and SQLite bytes on the real sprefa database before selecting retention or compaction policy.
