---
created: 2026-08-17
updated: 2026-08-17
type: feature
status: open
priority: normal
epic: boop-lane-observability
labels: [domain-boop, intent-observability, component-query]
size: M
---

# Agent tree over time, chromium-network-tab waterfall

## Description

An agent tree over time, rendered like the chromium network tab. Every agent or
lane is a row the way a websocket connection is a row; its lifespan is a bar;
its events (spawn, turn-start, turn-finish, mail sent, mail received, error,
exit) are frames on that bar; parent/child edges are drawn between rows.

Decided, not a fork. This card is BOOP'S HALF: what boop must expose so a
viewer can read lifespans, events and edges in ONE query. A separate navigator
owns the `hafley-rxjs` renderer half.

## The data that exists, `~/.agent/boop.db` (387 MB)

```sql
CREATE TABLE agent_trace_event (
  event_id INTEGER PRIMARY KEY, event_key TEXT NOT NULL UNIQUE,
  lane_id INTEGER NOT NULL REFERENCES dict_session(id),
  trace_id INTEGER REFERENCES dict_trace(id), session_id INTEGER REFERENCES dict_session(id),
  from_lane_id INTEGER REFERENCES dict_session(id), to_lane_id INTEGER REFERENCES dict_session(id),
  kind_id INTEGER NOT NULL REFERENCES dict_trace_kind(id),
  started_ts INTEGER, finished_ts INTEGER,
  delivery_state_id INTEGER REFERENCES dict_trace_delivery(id),
  classification_id INTEGER REFERENCES dict_trace_classification(id),
  detail TEXT NOT NULL DEFAULT '', created_ts INTEGER NOT NULL);

CREATE TABLE agent_lane (
  spawn_id INTEGER PRIMARY KEY, lane_id INTEGER NOT NULL, trace_id INTEGER, harness_id INTEGER,
  branch_id INTEGER, cwd_id INTEGER, model_id INTEGER, parent_lane_id INTEGER, goal TEXT,
  brief_path_id INTEGER, brief_markdown_id INTEGER, spawned_ts INTEGER NOT NULL);

CREATE TABLE agent_edge (
  parent_session_id INTEGER NOT NULL, child_session_id INTEGER NOT NULL, edge_kind_id INTEGER NOT NULL,
  agent_type_id INTEGER, model_id INTEGER, first_ts INTEGER, last_ts INTEGER, n INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (parent_session_id, child_session_id, edge_kind_id)) WITHOUT ROWID;

CREATE TABLE agent_turn (
  session_id INTEGER NOT NULL, turn INTEGER NOT NULL, ts INTEGER, role_id INTEGER NOT NULL, said TEXT,
  PRIMARY KEY (session_id, turn)) WITHOUT ROWID;

CREATE TABLE agent_span (
  session_id INTEGER NOT NULL, turn INTEGER NOT NULL, path_id INTEGER NOT NULL,
  line_start INTEGER, line_end INTEGER, PRIMARY KEY (session_id, turn, path_id)) WITHOUT ROWID;

CREATE TABLE agent_live_span (
  session_id INTEGER NOT NULL, from_ts INTEGER NOT NULL, to_ts INTEGER, status_id INTEGER NOT NULL,
  pid INTEGER, tmux_pane_id INTEGER, PRIMARY KEY (session_id, from_ts)) WITHOUT ROWID;
```

| table | rows | ts col | identity col | parent col |
|---|---|---|---|---|
| agent_trace_event | 246 | `created_ts` | `lane_id` | `from_lane_id` / `to_lane_id` |
| agent_lane | 254 (181 distinct lanes) | `spawned_ts` | `lane_id` | `parent_lane_id` |
| agent_edge | 1879 | `first_ts` / `last_ts` | `child_session_id` | `parent_session_id` |
| agent_turn | 449943 | `ts` | `session_id` | none |
| agent_span | 0 | | `session_id` | none |
| agent_live_span | 6771 (3581 sessions) | `from_ts` / `to_ts` | `session_id` | none |

Timestamps are epoch milliseconds INTEGER everywhere. Every id column is
dict-encoded. `dict_trace_kind` = supervisor-start, channel-open, turn-start,
error, supervisor-exit, turn-finish, an exact match for the frame vocabulary.
`dict_edekind` = spawned, result, deliver-nextturn, hail, deliver-midturn.
`dict_trace_delivery` is EMPTY; `dict_trace_classification` = starting, opened,
started, failed, completed.

Two traps:
1. `dict_session` holds two disjoint namespaces. `agent_lane.lane_id` resolves to lane NAMES; `agent_turn.session_id` resolves to harness uuids (`ses_fee7b1...`). Zero of 181 lanes have turns under their own id. The bridge is `agent_trace_span(session_id, trace_id, attach_id)`: one row `attach=lane-create` (lane name) and one `attach=supervisor-conversation` (uuid) per `trace_id`.
2. `agent_trace_event` covers only 2 distinct lanes. It is the newest and thinnest source. Per-lane frames at scale must come from `agent_turn` + `agent_live_span` + `agent_edge`.

## Renderer that exists, `~/projects/hafley-rxjs/packages/marbler`

Input row type (`src/0_types.ts`, zod-validated):
`MarbleEvent = {id, name, method, status: number, type, initiator, size, start: number|null, duration: number|null, from, to, preview, phases: MarblePhase[]}`;
`MarblePhase = {kind: "queue"|"send"|"wait"|"receive"|"work", start, end}`.

Renders today: DOM table on `@hafley66/grid` (`createGrid`, `src/1_model.ts`),
PixiJS waterfall (`src/1a_WaterfallPixi.tsx`, `WaterfallPixiProps = {rows, scroller, domain?, onEventHover?, onEventSelect?}`, ROW_HEIGHT 44),
density overview (`src/1b_TimeNavigatorPixi.tsx`). Caller supplies
`createMarbler(seed: MarbleEvent[])` and a scroller ref. `src/0a_TimeViewport.ts`
already exports `TimelineMark = {kind:"dot",time,lane,variant} | {kind:"span",start,end,lane} | {kind:"link",from,to}`,
so the parent/child edge primitive ALREADY EXISTS and is unused.

## Adapters that exist, `~/projects/hafley-rxjs/packages/boop-adapters/src`

| file | exports | expects |
|---|---|---|
| `0_types.ts` | `BOOP_AGENT_SNAPSHOT_VERSION = "boop-agent/1"`; `BoopAgentNode/Edge/Phase/Event/Snapshot`, `AgentTreeRow`, `AgentTreeCommunication`, `AgentTimelineEvent = MarbleEvent`, `AgentTopology`; `boopAgentSnapshotSchema` | `{schemaVersion, nodes[], edges[], events[]}`, node id `${harness}:${id}` |
| `1_validate.ts` | `parseBoopAgentSnapshot`, `nodeIdentity(h,id)`, `resolveIdentity(set,ref)`, `eventIdentity`, `timeOf`, `eventStart/eventEnd` | ms numbers or rfc3339, both accepted |
| `2_tree.ts` | `projectAgentTree(s): AgentTreeRow[]` | parent from `node.parentId` or edges with kind in {spawn, parent, subagent}. BUG: boop spells it `spawned`, so the filter drops every real edge |
| `3_timeline.ts` | `projectAgentTimeline(s): MarbleEvent[]` | maps kind to method SEND/RECV/SPAWN/WORK/DONE/ERROR |
| `4_topology.ts` | `projectAgentTopology(s)` | index-pair edges for `@hafley66/grapht` |
| `5_route.ts` | `boopAgentRoute = route('/agents/:harness/:sessionId')` | |

Missing for the waterfall: no lifespan type (a node has no `startTs`/`endTs`,
so a row bar cannot be drawn); no event-frame kind (the phase kinds are
HTTP-ish, no spawn/turn-start/turn-finish/exit/error member); rows are events
rather than agents, so N frames on one agent become N rows and there is no
`laneIndex` grouping key; no loader at all, nothing fetches a snapshot;
`TimelineMark.link` never reaches the renderer.

## Boop's half: the named query `agent-waterfall`

ONE result set with a `kind` discriminator column, not an ndjson of three
record types. `boop db "<sql>"` already emits one result set through
`run_passthrough` (`crates/boop/src/main.rs:5706`) with `--format`; a
three-record ndjson needs a new writer and a new schema contract, and the
`AgentSessionGraph` precedent shows the multi-member JSON path costs a whole
Rust struct set. Common column shape: `kind, row_id, lane, peer, t0, t1, label, detail`.

Verified to run read-only against the live store:

```sql
WITH lane_span AS (
  SELECT l.lane_id, MIN(l.spawned_ts) AS start_ts,
         MAX(COALESCE(ev.last_ts, ls.last_ts, l.spawned_ts)) AS end_ts,
         MAX(l.parent_lane_id) AS parent_lane_id, MAX(l.goal) AS goal, MAX(l.harness_id) AS harness_id
    FROM agent_lane l
    LEFT JOIN (SELECT lane_id, MAX(COALESCE(finished_ts, created_ts)) AS last_ts
                 FROM agent_trace_event GROUP BY lane_id) ev ON ev.lane_id = l.lane_id
    LEFT JOIN (SELECT session_id, MAX(COALESCE(to_ts, from_ts)) AS last_ts
                 FROM agent_live_span GROUP BY session_id) ls ON ls.session_id = l.lane_id
   WHERE l.spawned_ts >= :since GROUP BY l.lane_id)
SELECT 'lane' AS kind, s.value AS row_id, s.value AS lane, p.value AS peer,
       sp.start_ts AS t0, sp.end_ts AS t1, h.value AS label, sp.goal AS detail
  FROM lane_span sp JOIN dict_session s ON s.id = sp.lane_id
  LEFT JOIN dict_session p ON p.id = sp.parent_lane_id
  LEFT JOIN dict_harness h ON h.id = sp.harness_id
UNION ALL
SELECT 'event', e.event_key, s.value, COALESCE(f.value, t.value),
       COALESCE(e.started_ts, e.created_ts), e.finished_ts, k.value, e.detail
  FROM agent_trace_event e JOIN dict_session s ON s.id = e.lane_id
  JOIN dict_trace_kind k ON k.id = e.kind_id
  LEFT JOIN dict_session f ON f.id = e.from_lane_id
  LEFT JOIN dict_session t ON t.id = e.to_lane_id
 WHERE e.created_ts >= :since
UNION ALL
SELECT 'live', CAST(v.session_id AS TEXT) || '@' || CAST(v.from_ts AS TEXT), s.value, NULL,
       v.from_ts, v.to_ts, st.value, ''
  FROM agent_live_span v JOIN dict_session s ON s.id = v.session_id
  LEFT JOIN dict_status st ON st.id = v.status_id
 WHERE v.from_ts >= :since
UNION ALL
SELECT 'edge', pa.value || '>' || ch.value || '/' || ek.value, pa.value, ch.value,
       g.first_ts, g.last_ts, ek.value, CAST(g.n AS TEXT)
  FROM agent_edge g JOIN dict_session pa ON pa.id = g.parent_session_id
  JOIN dict_session ch ON ch.id = g.child_session_id
  JOIN dict_edekind ek ON ek.id = g.edge_kind_id
 WHERE COALESCE(g.last_ts, g.first_ts) >= :since
 ORDER BY t0;
```

Row semantics: `kind='lane'` is a row plus its lifespan bar, `peer` is the
parent row. `kind='event'` is a frame on that bar, `label` in
supervisor-start / channel-open / turn-start / turn-finish / error /
supervisor-exit. `kind='live'` is a status sub-bar for uuid sessions, the dense
fallback while `agent_trace_event` covers 2 lanes. `kind='edge'` is the
`TimelineMark.link` between rows.

Home: `crates/boop/src/_0_session_graph.rs`, the only existing named-SQL graph
report (`const SESSION_GRAPH_SQL` at line 103, with
`AGENT_SESSION_GRAPH_SCHEMA_VERSION` and typed row structs), re-exported from
`crates/boop/src/lib.rs:55-57`. CLI registration sits beside
`AgentSummaryCmd::Sessions` (`crates/boop/src/main.rs:4099-4111`, dispatched at
`:5811` / `:5834`): add `AgentSummaryCmd::Waterfall { since, cwd, format }`.
`query_trace_events` (`crates/boop/src/ident.rs:1002`) and `TraceEventRow`
(`ident.rs:141`) are the model if the report is typed rather than passthrough.

## Acceptance Criteria

- [ ] `boop beep agent waterfall --since <ms|duration> [--cwd <p>] --format json|ndjson|table` exists and returns the four `kind` classes in one result set.
- [ ] The SQL lives as a named constant beside `SESSION_GRAPH_SQL` in `_0_session_graph.rs`; no interpolated values, `:since` and `:cwd` are bound.
- [ ] The read opens the store READ-ONLY (see `@boop-db-readonly-open`).
- [ ] Output over the live store is under 10 seconds for a 24 h window; the measured number is in the PR body.
- [ ] EXPLAIN QUERY PLAN shows SEARCH rather than SCAN on `agent_trace_event` (`idx_trace_event_lane_time`) and `agent_lane` (`idx_lane_lane`); pasted in the PR body.
- [ ] A schema-version constant travels with the payload so the TS side can reject a mismatch.
- [ ] Documented: the `dict_session` two-namespace trap and the `agent_trace_span` bridge, so the viewer knows why a lane row has no turns.
- [ ] Reported to the hafley-rxjs navigator: `2_tree.ts` filters on edge kind `spawn` while boop emits `spawned`, dropping every real edge.

## Tests Run

## Implementation Notes

Read-only scouting; nothing was changed to produce this card.
