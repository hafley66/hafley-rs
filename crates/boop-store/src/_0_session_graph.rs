//! Typed projection of native harness sessions and shell-only lanes.

use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::bus::{Message, Route};
use crate::proc::ProcReader;
use crate::runtime::{runtime_snapshot, AgentRuntimeRow, RuntimeSnapshotInput};
use crate::tmux::Multiplexer;
use crate::{Store, TraceEventRow};

/// Version of the JSON session-graph document.
///
/// `trace_events` is an additive member of schema version 1. It has a serde
/// default so a version-1 document produced before this member existed still
/// deserializes with an empty event list.
pub const AGENT_SESSION_GRAPH_SCHEMA_VERSION: u32 = 1;

/// Maximum number of trace events exposed by one graph document.
const AGENT_SESSION_GRAPH_TRACE_EVENT_LIMIT: u64 = 1_000;

/// Filters for one session-graph read.
#[derive(Clone, Debug, Default)]
pub struct AgentSessionGraphQuery {
    pub cwd: Option<PathBuf>,
    pub include_history: bool,
    /// Exact tmux session or pane evidence for one focused shell. This is an
    /// observed transport identity, never a cwd or transcript-path heuristic.
    pub tmux: Option<String>,
    /// Include completed family members whose lifecycle activity is at or
    /// after this epoch-millisecond boundary. Roots remain present to keep the
    /// returned family connected.
    pub history_since_ts: Option<u64>,
}

/// Function type for the pure durable graph projection.
pub type LoadAgentSessionGraph = fn(&Store, AgentSessionGraphQuery) -> Result<AgentSessionGraph>;

/// Harness-qualified public identity. The store currently keys sessions by
/// the bare `dict_session` value, so a collision that already merged rows in
/// storage cannot be reconstructed by this projection.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct AgentSessionIdentity {
    pub harness: String,
    pub id: String,
}

/// The complete native-session and shell projection.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct AgentSessionGraph {
    pub schema_version: u32,
    pub sessions: Vec<AgentSessionNode>,
    pub edges: Vec<AgentSessionEdge>,
    pub shells: Vec<AgentShellNode>,
    /// Events whose lane belongs to one of the selected `sessions` or
    /// `shells`. Older schema-version-1 documents may omit this member.
    #[serde(default)]
    pub trace_events: Vec<TraceEventRow>,
}

/// One normalized harness transcript session.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct AgentSessionNode {
    pub session: AgentSessionIdentity,
    pub cwd: Option<PathBuf>,
    pub tmux: Option<String>,
    pub state: Option<String>,
    #[serde(default)]
    pub trace: Option<String>,
    #[serde(default)]
    pub trace_attached_ts: Option<u64>,
    #[serde(default)]
    pub started_ts: Option<u64>,
    pub last_activity_ts: Option<u64>,
    #[serde(default)]
    pub finished_ts: Option<u64>,
}

/// One native parent-child session relation.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct AgentSessionEdge {
    pub parent: AgentSessionIdentity,
    pub child: AgentSessionIdentity,
    pub kind: String,
    #[serde(default)]
    pub first_ts: Option<u64>,
    #[serde(default)]
    pub last_ts: Option<u64>,
}

/// One registered lane with no harness transcript session.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct AgentShellNode {
    pub lane: String,
    pub parent_lane: Option<String>,
    pub harness: Option<String>,
    pub mode: Option<String>,
    pub session_id: Option<String>,
    /// The stable native identity named by a harness-backed route. It is a
    /// reference to `sessions`, rather than a second session record.
    #[serde(default)]
    pub session: Option<AgentSessionIdentity>,
    #[serde(default)]
    pub trace: Option<String>,
    pub cwd: Option<PathBuf>,
    pub tmux: Option<String>,
    #[serde(default)]
    pub tmux_session: Option<String>,
    #[serde(default)]
    pub tmux_pane: Option<String>,
    pub pid: Option<u32>,
    pub state: String,
    #[serde(default)]
    pub started_ts: Option<u64>,
    #[serde(default)]
    pub registered_at: Option<String>,
}

/// Runtime inputs for the production projection. The process table and tmux
/// listing are supplied by the caller so the complete request takes one
/// bounded observation of each external runtime source.
pub struct AgentSessionGraphRuntime<'a> {
    pub routes: &'a std::collections::BTreeMap<String, Route>,
    pub messages: &'a [Message],
    pub multiplexer: &'a dyn Multiplexer,
    pub tmux_socket: Option<&'a str>,
    pub processes: &'a dyn ProcReader,
}

/// Scope native sessions before reading their transcript and usage activity.
/// The aggregate relations only touch session ids emitted by the graph filter,
/// rather than grouping every row in the local corpus.
const SESSION_GRAPH_SQL: &str = r#"
WITH scoped_sessions AS MATERIALIZED (
    SELECT a.session_id,
           s.value AS session,
           h.value AS harness,
           c.value AS cwd,
           p.value AS tmux,
           st.value AS state,
           trace.value AS trace,
           span.attached_ts,
           a.started_ts,
           (SELECT MAX(dead.from_ts) FROM agent_live_span dead
             JOIN dict_status dead_status ON dead_status.id = dead.status_id
            WHERE dead.session_id = a.session_id AND dead_status.value = 'dead') AS finished_ts
      FROM agent_session a
      JOIN dict_session s ON s.id = a.session_id
      JOIN dict_harness h ON h.id = a.harness_id
      LEFT JOIN dict_cwd c ON c.id = a.cwd_id
      LEFT JOIN agent_live live ON live.session_id = a.session_id
      LEFT JOIN dict_pane p ON p.id = live.tmux_pane_id
      LEFT JOIN dict_status st ON st.id = live.status_id
      LEFT JOIN agent_trace_span span ON span.session_id = a.session_id
      LEFT JOIN dict_trace trace ON trace.id = span.trace_id
     WHERE (?1 IS NULL OR c.value = ?1)
       AND (?2 OR st.value IS NULL OR st.value <> 'dead')
),
turns AS (
    SELECT t.session_id, MAX(t.ts) AS last_ts
      FROM agent_turn t
     WHERE t.session_id IN (SELECT session_id FROM scoped_sessions)
     GROUP BY t.session_id
),
usage AS (
    SELECT u.session_id, MAX(u.ts) AS last_ts
      FROM agent_usage u
     WHERE u.session_id IN (SELECT session_id FROM scoped_sessions)
     GROUP BY u.session_id
)
SELECT scoped.session,
       scoped.harness,
       scoped.cwd,
       scoped.tmux,
       scoped.state,
       CASE WHEN turns.last_ts IS NULL AND usage.last_ts IS NULL
            THEN NULL
            ELSE MAX(COALESCE(turns.last_ts, 0), COALESCE(usage.last_ts, 0))
       END AS last_ts,
       scoped.trace, scoped.attached_ts, scoped.started_ts, scoped.finished_ts
  FROM scoped_sessions scoped
  LEFT JOIN turns ON turns.session_id = scoped.session_id
  LEFT JOIN usage ON usage.session_id = scoped.session_id
 ORDER BY scoped.session
"#;

/// Load session nodes, native edges, and durable shell rows with set-wise
/// store reads. No runtime acquisition occurs in this function.
pub fn load_agent_session_graph(
    store: &Store,
    query: AgentSessionGraphQuery,
) -> Result<AgentSessionGraph> {
    let cwd = query
        .tmux
        .is_none()
        .then_some(query.cwd.as_ref())
        .flatten()
        .map(|path| path.to_string_lossy().into_owned());
    let history = query.include_history || query.history_since_ts.is_some();
    let mut statement = store.connection().prepare(SESSION_GRAPH_SQL)?;
    let rows = statement.query_map(rusqlite::params![cwd, history], |row| {
        Ok(AgentSessionNode {
            session: AgentSessionIdentity {
                id: row.get(0)?,
                harness: row.get(1)?,
            },
            cwd: row.get::<_, Option<String>>(2)?.map(PathBuf::from),
            tmux: row.get(3)?,
            state: row.get(4)?,
            trace: row.get(6)?,
            trace_attached_ts: row
                .get::<_, Option<i64>>(7)?
                .and_then(|value| u64::try_from(value).ok()),
            started_ts: row
                .get::<_, Option<i64>>(8)?
                .and_then(|value| u64::try_from(value).ok()),
            last_activity_ts: row
                .get::<_, Option<i64>>(5)?
                .and_then(|value| u64::try_from(value).ok()),
            finished_ts: row
                .get::<_, Option<i64>>(9)?
                .and_then(|value| u64::try_from(value).ok()),
        })
    })?;
    let sessions = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let included = sessions
        .iter()
        .map(|session| session.session.id.as_str())
        .collect::<BTreeSet<_>>();
    let edge_sql = "SELECT p.value, c.value, hp.value, hc.value, k.value, e.first_ts, e.last_ts
                      FROM agent_edge e
                      JOIN dict_session p ON p.id = e.parent_session_id
                      JOIN dict_session c ON c.id = e.child_session_id
                      JOIN agent_session ap ON ap.session_id = e.parent_session_id
                      JOIN agent_session ac ON ac.session_id = e.child_session_id
                      JOIN dict_harness hp ON hp.id = ap.harness_id
                      JOIN dict_harness hc ON hc.id = ac.harness_id
                      JOIN dict_edekind k ON k.id = e.edge_kind_id
                     ORDER BY p.value, c.value, k.value";
    let mut statement = store.connection().prepare(edge_sql)?;
    let edges = statement
        .query_map([], |row| {
            Ok(AgentSessionEdge {
                parent: AgentSessionIdentity {
                    id: row.get(0)?,
                    harness: row.get(2)?,
                },
                child: AgentSessionIdentity {
                    id: row.get(1)?,
                    harness: row.get(3)?,
                },
                kind: row.get(4)?,
                first_ts: row
                    .get::<_, Option<i64>>(5)?
                    .and_then(|value| u64::try_from(value).ok()),
                last_ts: row
                    .get::<_, Option<i64>>(6)?
                    .and_then(|value| u64::try_from(value).ok()),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|edge| {
            included.contains(edge.parent.id.as_str()) && included.contains(edge.child.id.as_str())
        })
        .collect::<Vec<_>>();

    let shell_sql = "SELECT lane.value, parent.value, trace.value, cwd.value, pane.value,
                            live.pid, COALESCE(status.value, 'unknown'), lane_row.spawned_ts
                       FROM agent_lane lane_row
                       JOIN dict_session lane ON lane.id = lane_row.lane_id
                       LEFT JOIN dict_session parent ON parent.id = lane_row.parent_lane_id
                       LEFT JOIN dict_trace trace ON trace.id = lane_row.trace_id
                       LEFT JOIN dict_cwd cwd ON cwd.id = lane_row.cwd_id
                       LEFT JOIN agent_live live ON live.session_id = lane_row.lane_id
                       LEFT JOIN dict_pane pane ON pane.id = live.tmux_pane_id
                       LEFT JOIN dict_status status ON status.id = live.status_id
                      WHERE (?1 IS NULL OR cwd.value = ?1)
                        AND (?2 OR status.value = 'live')
                        AND lane_row.harness_id IS NULL
                      ORDER BY lane.value, lane_row.spawned_ts DESC";
    let mut statement = store.connection().prepare(shell_sql)?;
    let mut shells = Vec::new();
    let mut seen_lanes = BTreeSet::new();
    for row in statement.query_map(rusqlite::params![cwd, history], |row| {
        Ok(AgentShellNode {
            lane: row.get(0)?,
            parent_lane: row.get(1)?,
            harness: None,
            mode: None,
            session_id: None,
            session: None,
            trace: row.get(2)?,
            cwd: row.get::<_, Option<String>>(3)?.map(PathBuf::from),
            tmux: row.get(4)?,
            tmux_session: None,
            tmux_pane: None,
            pid: row
                .get::<_, Option<i64>>(5)?
                .and_then(|pid| u32::try_from(pid).ok()),
            state: row.get(6)?,
            started_ts: row
                .get::<_, Option<i64>>(7)?
                .and_then(|value| u64::try_from(value).ok()),
            registered_at: None,
        })
    })? {
        let shell = row?;
        if !included.contains(shell.lane.as_str()) && seen_lanes.insert(shell.lane.clone()) {
            shells.push(shell);
        }
    }

    let mut graph = AgentSessionGraph {
        schema_version: AGENT_SESSION_GRAPH_SCHEMA_VERSION,
        sessions,
        edges,
        shells,
        trace_events: Vec::new(),
    };
    focus_graph(&mut graph, &query);
    graph.trace_events = query_trace_events(store, &graph.sessions, &graph.shells)?;
    Ok(graph)
}

/// Query the bounded event surface for exactly the lanes selected by the
/// graph's cwd and history filters. `Store::query_trace_events` accepts one
/// lane at a time, so the per-lane reads are merged and capped again here.
fn query_trace_events(
    store: &Store,
    sessions: &[AgentSessionNode],
    shells: &[AgentShellNode],
) -> Result<Vec<TraceEventRow>> {
    let lanes = sessions
        .iter()
        .map(|session| session.session.id.clone())
        .chain(shells.iter().map(|shell| shell.lane.clone()))
        .collect::<BTreeSet<_>>();
    let mut events = Vec::new();
    for lane in lanes {
        events
            .extend(store.query_trace_events(Some(&lane), AGENT_SESSION_GRAPH_TRACE_EVENT_LIMIT)?);
    }
    events.sort_by(|left, right| {
        left.created_ts
            .cmp(&right.created_ts)
            .then_with(|| left.event_key.cmp(&right.event_key))
    });
    events.truncate(AGENT_SESSION_GRAPH_TRACE_EVENT_LIMIT as usize);
    Ok(events)
}

/// Load the durable graph and merge one bounded tmux/process observation.
pub fn load_agent_session_graph_with_runtime(
    store: &Store,
    query: AgentSessionGraphQuery,
    runtime: AgentSessionGraphRuntime<'_>,
) -> Result<AgentSessionGraph> {
    let include_history = query.include_history || query.history_since_ts.is_some();
    let cwd = query
        .tmux
        .is_none()
        .then_some(query.cwd.as_ref())
        .flatten()
        .map(|path| path.to_string_lossy().into_owned());
    // Runtime routes carry the tmux-to-native-session anchor. Keep the durable
    // component intact until those route shells have been merged, then focus
    // exactly once below.
    let mut durable_query = query.clone();
    durable_query.tmux = None;
    let mut graph = load_agent_session_graph(store, durable_query)?;
    let rows = runtime_snapshot(RuntimeSnapshotInput {
        store,
        routes: runtime.routes,
        messages: runtime.messages,
        multiplexer: runtime.multiplexer,
        tmux_socket: runtime.tmux_socket,
        processes: runtime.processes,
    })?;
    for row in rows {
        if let Some(mut shell) = shell_from_runtime(row) {
            if let Some(pane) = shell
                .tmux
                .as_deref()
                .filter(|target| target.starts_with('%'))
            {
                shell.tmux_pane = Some(pane.to_owned());
                shell.tmux_session = runtime
                    .multiplexer
                    .session_of_pane(runtime.tmux_socket, pane);
                if runtime.multiplexer.target_alive(runtime.tmux_socket, pane) {
                    shell.state = "live".to_owned();
                }
            }
            if !include_history && shell.state != "live" {
                continue;
            }
            if let Some(cwd) = cwd.as_deref() {
                let shell_matches_cwd = shell
                    .cwd
                    .as_ref()
                    .map(|path| path.to_string_lossy() == cwd)
                    .unwrap_or(false);
                if !shell_matches_cwd {
                    continue;
                }
            }
            if let Some(existing) = graph
                .shells
                .iter_mut()
                .find(|existing| existing.lane == shell.lane)
            {
                *existing = shell;
            } else {
                graph.shells.push(shell);
            }
        }
    }
    graph
        .shells
        .sort_by(|left, right| left.lane.cmp(&right.lane));
    focus_graph(&mut graph, &query);
    graph.trace_events = query_trace_events(store, &graph.sessions, &graph.shells)?;
    Ok(graph)
}

fn shell_from_runtime(row: AgentRuntimeRow) -> Option<AgentShellNode> {
    let route = row.route?;
    let tmux = row.tmux_target.clone().or(row.tmux_pane.clone())?;
    let state = if matches!(row.liveness.process, crate::runtime::ProcessLiveness::Live)
        || matches!(row.liveness.tmux, crate::runtime::TmuxLiveness::Live)
    {
        "live"
    } else if matches!(row.liveness.process, crate::runtime::ProcessLiveness::Dead)
        || matches!(row.liveness.tmux, crate::runtime::TmuxLiveness::Dead)
    {
        "dead"
    } else {
        row.reported_status.as_deref().unwrap_or("unknown")
    };
    Some(AgentShellNode {
        lane: row.lane,
        parent_lane: route.parent,
        harness: route.harness.map(|id| id.as_str().to_owned()),
        mode: route.mode.clone(),
        session: route
            .session_id
            .as_ref()
            .zip(route.harness)
            .map(|(id, harness)| AgentSessionIdentity {
                harness: harness.as_str().to_owned(),
                id: id.clone(),
            }),
        session_id: route.session_id,
        trace: row.trace,
        cwd: row.cwd.map(PathBuf::from),
        tmux: Some(tmux),
        tmux_session: row
            .tmux_target
            .as_deref()
            .and_then(tmux_session_anchor)
            .map(str::to_owned),
        tmux_pane: row.tmux_pane.filter(|target| target.starts_with('%')),
        pid: row.pid.and_then(|pid| u32::try_from(pid).ok()),
        state: state.to_owned(),
        started_ts: None,
        registered_at: route.registered_at,
    })
}
/// Reduce a broad durable projection to the rooted family selected by exact
/// tmux evidence. `spawned` is the only edge kind used as parenthood: hail and
/// delivery edges stay visible when both endpoints are in the family but never
/// create ancestry.
fn focus_graph(graph: &mut AgentSessionGraph, query: &AgentSessionGraphQuery) {
    let Some(tmux) = query.tmux.as_deref() else {
        return;
    };
    let query_session = tmux_session_anchor(tmux);
    let mut lanes = graph
        .shells
        .iter()
        .filter(|shell| {
            shell.tmux.as_deref() == Some(tmux)
                || shell.tmux_session.as_deref() == Some(tmux)
                || shell.tmux_pane.as_deref() == Some(tmux)
                || query_session.is_some_and(|session| {
                    shell.tmux_session.as_deref() == Some(session)
                        || shell.tmux.as_deref().and_then(tmux_session_anchor) == Some(session)
                })
        })
        .map(|shell| shell.lane.clone())
        .collect::<BTreeSet<_>>();
    let mut sessions = graph
        .sessions
        .iter()
        .filter(|session| {
            session.tmux.as_deref() == Some(tmux)
                || query_session.is_some_and(|anchor| {
                    session.tmux.as_deref().and_then(tmux_session_anchor) == Some(anchor)
                })
        })
        .map(|session| session.session.clone())
        .collect::<BTreeSet<_>>();
    for shell in &graph.shells {
        if lanes.contains(&shell.lane) {
            if let Some(session) = &shell.session {
                sessions.insert(session.clone());
            }
        }
    }
    loop {
        let mut changed = false;
        for shell in &graph.shells {
            if lanes.contains(&shell.lane) {
                if let Some(parent) = &shell.parent_lane {
                    changed |= lanes.insert(parent.clone());
                }
            }
            if shell
                .parent_lane
                .as_ref()
                .is_some_and(|parent| lanes.contains(parent))
            {
                changed |= lanes.insert(shell.lane.clone());
            }
        }
        for edge in graph.edges.iter().filter(|edge| edge.kind == "spawned") {
            if sessions.contains(&edge.child) {
                changed |= sessions.insert(edge.parent.clone());
            }
            if sessions.contains(&edge.parent) {
                changed |= sessions.insert(edge.child.clone());
            }
        }
        for shell in &graph.shells {
            if lanes.contains(&shell.lane) {
                if let Some(session) = &shell.session {
                    changed |= sessions.insert(session.clone());
                }
            }
        }
        if !changed {
            break;
        }
    }
    if let Some(since) = query.history_since_ts {
        let roots = sessions
            .iter()
            .filter(|identity| {
                !graph.edges.iter().any(|edge| {
                    edge.kind == "spawned"
                        && edge.child == **identity
                        && sessions.contains(&edge.parent)
                })
            })
            .cloned()
            .collect::<BTreeSet<_>>();
        sessions.retain(|identity| {
            graph
                .sessions
                .iter()
                .find(|node| &node.session == identity)
                .map(|node| {
                    roots.contains(identity)
                        || node
                            .last_activity_ts
                            .or(node.finished_ts)
                            .or(node.started_ts)
                            .is_some_and(|ts| ts >= since)
                })
                .unwrap_or(false)
        });
    }
    graph
        .sessions
        .retain(|node| sessions.contains(&node.session));
    graph.shells.retain(|shell| lanes.contains(&shell.lane));
    graph
        .edges
        .retain(|edge| sessions.contains(&edge.parent) && sessions.contains(&edge.child));
}

/// The session component of a tmux target. `%pane` remains a distinct exact
/// identity because its owning session is runtime evidence, not syntax.
fn tmux_session_anchor(target: &str) -> Option<&str> {
    (!target.starts_with('%')).then(|| target.split(':').next().unwrap_or(target))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::harness_id::HarnessId;
    use crate::ident::{LaneSpawn, TraceEvent};
    use crate::proc::SysinfoSnapshot;
    use crate::runtime::{ProcessLiveness, ResolvedRoute, RuntimeLiveness, TmuxLiveness};
    use crate::testing::FakeMux;

    #[test]
    fn graph_projects_sessions_edges_and_shells_from_setwise_relations() {
        let path =
            std::env::temp_dir().join(format!("boop-session-graph-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let store = Store::open(path.clone()).unwrap();
        store
            .record_lane_spawn(&LaneSpawn {
                lane: "shell".into(),
                parent: Some("parent-lane".into()),
                cwd: Some("/repo".into()),
                ts: 1,
                ..LaneSpawn::default()
            })
            .unwrap();
        let parent = store.intern_public("dict_session", "parent").unwrap();
        let child = store.intern_public("dict_session", "child").unwrap();
        let harness = store.intern_public("dict_harness", "codex").unwrap();
        store
            .connection()
            .execute(
                "INSERT INTO agent_session(session_id, harness_id, cwd_id) VALUES (?1, ?2, NULL), (?3, ?2, NULL)",
                rusqlite::params![parent, harness, child],
            )
            .unwrap();
        store.add_edge_at("parent", "child", "spawned", 1).unwrap();
        let graph = load_agent_session_graph(
            &store,
            AgentSessionGraphQuery {
                cwd: None,
                include_history: true,
                ..AgentSessionGraphQuery::default()
            },
        )
        .unwrap();
        assert_eq!(graph.sessions.len(), 2);
        assert_eq!(graph.edges[0].kind, "spawned");
        assert_eq!(graph.shells[0].lane, "shell");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn current_graph_keeps_discovered_native_sessions_with_idle_status() {
        let path =
            std::env::temp_dir().join(format!("boop-session-graph-idle-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let store = Store::open(path.clone()).unwrap();
        let session = store.intern_public("dict_session", "idle-native").unwrap();
        let harness = store.intern_public("dict_harness", "claude").unwrap();
        store
            .connection()
            .execute(
                "INSERT INTO agent_session(session_id, harness_id) VALUES (?1, ?2)",
                rusqlite::params![session, harness],
            )
            .unwrap();
        store
            .record_status("idle-native", 1, "idle", None, None)
            .unwrap();
        let graph = load_agent_session_graph(
            &store,
            AgentSessionGraphQuery {
                cwd: None,
                include_history: false,
                ..AgentSessionGraphQuery::default()
            },
        )
        .unwrap();
        assert_eq!(graph.sessions.len(), 1);
        assert_eq!(graph.sessions[0].state.as_deref(), Some("idle"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn scoped_activity_uses_the_latest_turn_or_usage_timestamp() {
        let path = std::env::temp_dir().join(format!(
            "boop-session-graph-last-activity-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let store = Store::open(path.clone()).unwrap();
        let session = store.intern_public("dict_session", "active").unwrap();
        let harness = store.intern_public("dict_harness", "codex").unwrap();
        let role = store.intern_public("dict_role", "assistant").unwrap();
        let model = store.intern_public("dict_model", "model").unwrap();
        store
            .connection()
            .execute(
                "INSERT INTO agent_session(session_id, harness_id) VALUES (?1, ?2)",
                rusqlite::params![session, harness],
            )
            .unwrap();
        store
            .connection()
            .execute(
                "INSERT INTO agent_turn(session_id, turn, ts, role_id, said) VALUES (?1, 1, 20, ?2, '')",
                rusqlite::params![session, role],
            )
            .unwrap();
        store
            .connection()
            .execute(
                "INSERT INTO dict_request(message_id, request_id) VALUES ('message', 'request')",
                [],
            )
            .unwrap();
        let request: i64 = store
            .connection()
            .query_row(
                "SELECT id FROM dict_request WHERE message_id = 'message' AND request_id = 'request'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        store
            .connection()
            .execute(
                "INSERT INTO agent_usage(session_id, turn, ts, request_ref, model_id) VALUES (?1, 1, 30, ?2, ?3)",
                rusqlite::params![session, request, model],
            )
            .unwrap();
        let graph = load_agent_session_graph(&store, AgentSessionGraphQuery::default()).unwrap();
        assert_eq!(graph.sessions[0].last_activity_ts, Some(30));
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn scoped_graph_activity_plan_avoids_whole_corpus_aggregates() {
        let path =
            std::env::temp_dir().join(format!("boop-session-graph-plan-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let store = Store::open(path.clone()).unwrap();
        let session = store.intern_public("dict_session", "scoped").unwrap();
        let harness = store.intern_public("dict_harness", "codex").unwrap();
        let cwd = store.intern_public("dict_cwd", "/scoped").unwrap();
        store
            .connection()
            .execute(
                "INSERT INTO agent_session(session_id, harness_id, cwd_id) VALUES (?1, ?2, ?3)",
                rusqlite::params![session, harness, cwd],
            )
            .unwrap();
        let plan_sql = format!("EXPLAIN QUERY PLAN {SESSION_GRAPH_SQL}");
        let mut statement = store.connection().prepare(&plan_sql).unwrap();
        let plan = statement
            .query_map(rusqlite::params!["/scoped", false], |row| {
                row.get::<_, String>(3)
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
            .join("\n");
        assert!(
            !plan.contains("SCAN agent_turn") && !plan.contains("SCAN t"),
            "turn aggregate scans the corpus:\n{plan}"
        );
        assert!(
            !plan.contains("SCAN agent_usage") && !plan.contains("SCAN u"),
            "usage aggregate scans the corpus:\n{plan}"
        );
        assert!(
            plan.contains("SEARCH t") && plan.contains("SEARCH u"),
            "scoped aggregate lookups missing:\n{plan}"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn unmatched_harness_routes_project_as_shell_nodes() {
        let route = ResolvedRoute {
            lane: "lane".into(),
            kind: "lane".into(),
            harness: Some(HarnessId::Codex),
            tmux: Some("lane".into()),
            cwd: Some("/repo".into()),
            model: None,
            mode: Some("auto".into()),
            session_id: Some("native".into()),
            source_path: None,
            parent: None,
            goal: None,
            registered_at: None,
        };
        let native = AgentRuntimeRow {
            lane: "lane".into(),
            trace: None,
            root_session: None,
            session: Some("native".into()),
            parent: None,
            route: Some(route.clone()),
            cwd: Some("/repo".into()),
            tmux_target: Some("lane".into()),
            tmux_pane: None,
            pid: None,
            reported_status: Some("live".into()),
            liveness: RuntimeLiveness {
                tmux: TmuxLiveness::Live,
                process: ProcessLiveness::Unknown,
            },
            completion: None,
            mailbox: Default::default(),
            worktree: Default::default(),
            diagnostics: Vec::new(),
        };
        let shell = shell_from_runtime(native).unwrap();
        assert_eq!(shell.lane, "lane");
        assert_eq!(shell.harness.as_deref(), Some("codex"));
        assert_eq!(shell.tmux.as_deref(), Some("lane"));
        assert_eq!(shell.session_id.as_deref(), Some("native"));
        let mut shell_route = route;
        shell_route.kind = "shell".into();
        shell_route.harness = None;
        let shell = AgentRuntimeRow {
            lane: "shell".into(),
            route: Some(shell_route),
            tmux_target: Some("shell".into()),
            ..native_for_shell()
        };
        let shell = shell_from_runtime(shell).unwrap();
        assert_eq!(shell.mode.as_deref(), Some("auto"));
        assert_eq!(shell.harness, None);
        assert_eq!(shell.session_id.as_deref(), Some("native"));
    }

    #[test]
    fn public_graph_projects_a_live_harness_coordinator_without_a_transcript() {
        let path = std::env::temp_dir().join(format!(
            "boop-session-graph-route-only-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let store = Store::open(path.clone()).unwrap();
        let mut routes = BTreeMap::new();
        routes.insert(
            "codex-1206".into(),
            Route {
                kind: "coordinator".into(),
                harness: Some(HarnessId::Codex),
                tmux: Some("codex-parent".into()),
                cwd: Some("/repo".into()),
                model: None,
                mode: Some("interactive".into()),
                session_id: Some("thread-codex-parent".into()),
                source_path: None,
                parent: None,
                goal: None,
                registered_at: Some("2026-08-18T00:00:00Z".into()),
                base_sha: None,
                worktree_dir: None,
                app_server_socket: None,
            },
        );
        let mux = FakeMux::available(&["codex-parent"]);
        let processes = SysinfoSnapshot::capture().unwrap();

        let graph = load_agent_session_graph_with_runtime(
            &store,
            AgentSessionGraphQuery {
                cwd: None,
                include_history: false,
                ..AgentSessionGraphQuery::default()
            },
            AgentSessionGraphRuntime {
                routes: &routes,
                messages: &[],
                multiplexer: &mux,
                tmux_socket: None,
                processes: &processes,
            },
        )
        .unwrap();

        assert!(graph.sessions.is_empty());
        assert_eq!(
            serde_json::to_value(&graph).unwrap()["shells"],
            serde_json::json!([{
                "lane": "codex-1206",
                "parent_lane": null,
                "harness": "codex",
                "mode": "interactive",
                "session_id": "thread-codex-parent",
                "session": {"harness": "codex", "id": "thread-codex-parent"},
                "trace": null,
                "cwd": "/repo",
                "tmux": "codex-parent",
                "tmux_session": "codex-parent",
                "tmux_pane": null,
                "pid": null,
                "state": "live",
                "started_ts": null,
                "registered_at": "2026-08-18T00:00:00Z"
            }])
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn focused_runtime_route_seeds_its_native_session_component() {
        let path = std::env::temp_dir().join(format!(
            "boop-session-graph-runtime-anchor-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let store = Store::open(path.clone()).unwrap();
        let harness = store.intern_public("dict_harness", "claude").unwrap();
        let parent = store
            .intern_public("dict_session", "claude-parent")
            .unwrap();
        let child = store.intern_public("dict_session", "claude-child").unwrap();
        store
            .connection()
            .execute(
                "INSERT INTO agent_session(session_id, harness_id) VALUES (?1, ?3), (?2, ?3)",
                rusqlite::params![parent, child, harness],
            )
            .unwrap();
        store
            .add_edge_at("claude-parent", "claude-child", "spawned", 10)
            .unwrap();
        let mut routes = BTreeMap::new();
        routes.insert(
            "claude-coordinator".into(),
            Route {
                kind: "coordinator".into(),
                harness: Some(HarnessId::Claude),
                tmux: Some("%1206".into()),
                cwd: Some("/repo".into()),
                model: None,
                mode: None,
                session_id: Some("claude-parent".into()),
                source_path: None,
                parent: None,
                goal: None,
                registered_at: None,
                base_sha: None,
                worktree_dir: None,
                app_server_socket: None,
            },
        );
        let mux = FakeMux::available(&["sprefa-5"]).with_pane("%1206", "sprefa-5");
        let processes = SysinfoSnapshot::capture().unwrap();
        let graph = load_agent_session_graph_with_runtime(
            &store,
            AgentSessionGraphQuery {
                tmux: Some("sprefa-5".into()),
                include_history: true,
                ..AgentSessionGraphQuery::default()
            },
            AgentSessionGraphRuntime {
                routes: &routes,
                messages: &[],
                multiplexer: &mux,
                tmux_socket: None,
                processes: &processes,
            },
        )
        .unwrap();

        assert_eq!(
            graph
                .sessions
                .iter()
                .map(|node| node.session.id.as_str())
                .collect::<Vec<_>>(),
            ["claude-child", "claude-parent"]
        );
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.shells.len(), 1);
        assert_eq!(graph.shells[0].tmux_pane.as_deref(), Some("%1206"));
        assert_eq!(graph.shells[0].tmux_session.as_deref(), Some("sprefa-5"));
        assert_eq!(graph.shells[0].state, "live");
        let _ = std::fs::remove_file(path);
    }

    fn native_for_shell() -> AgentRuntimeRow {
        AgentRuntimeRow {
            lane: "shell".into(),
            trace: None,
            root_session: None,
            session: None,
            parent: None,
            route: None,
            cwd: Some("/repo".into()),
            tmux_target: None,
            tmux_pane: None,
            pid: None,
            reported_status: Some("live".into()),
            liveness: RuntimeLiveness {
                tmux: TmuxLiveness::Live,
                process: ProcessLiveness::Unknown,
            },
            completion: None,
            mailbox: Default::default(),
            worktree: Default::default(),
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn qualified_identity_preserves_harness_for_distinct_native_rows() {
        let path = std::env::temp_dir().join(format!(
            "boop-session-graph-harnesses-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let store = Store::open(path.clone()).unwrap();
        for (harness_name, parent_name, child_name) in [
            ("claude", "claude-parent", "claude-child"),
            ("codex", "codex-parent", "codex-child"),
            ("opencode", "opencode-parent", "opencode-child"),
            ("kimi", "kimi-parent", "kimi-child"),
        ] {
            let harness = store.intern_public("dict_harness", harness_name).unwrap();
            let parent = store.intern_public("dict_session", parent_name).unwrap();
            let child = store.intern_public("dict_session", child_name).unwrap();
            store
                .connection()
                .execute(
                    "INSERT INTO agent_session(session_id, harness_id) VALUES (?1, ?2), (?3, ?2)",
                    rusqlite::params![parent, harness, child],
                )
                .unwrap();
            store
                .add_edge_at(parent_name, child_name, "spawned", 1)
                .unwrap();
        }
        let graph = load_agent_session_graph(
            &store,
            AgentSessionGraphQuery {
                cwd: None,
                include_history: true,
                ..AgentSessionGraphQuery::default()
            },
        )
        .unwrap();
        assert_eq!(graph.sessions.len(), 8);
        assert_eq!(graph.edges.len(), 4);
        assert!(graph.edges.iter().all(|edge| edge.kind == "spawned"));
        let identities = graph
            .sessions
            .iter()
            .map(|session| session.session.clone())
            .collect::<BTreeSet<_>>();
        assert!(graph
            .edges
            .iter()
            .all(|edge| identities.contains(&edge.parent) && identities.contains(&edge.child)));
        assert!(graph
            .sessions
            .iter()
            .any(|session| session.session.harness == HarnessId::Claude.as_str()));
        assert!(graph
            .sessions
            .iter()
            .any(|session| session.session.harness == HarnessId::Opencode.as_str()));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn graph_json_contains_trace_event_fixture_and_applies_cwd_and_history_filters() {
        let path = std::env::temp_dir().join(format!(
            "boop-session-graph-trace-filter-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let store = Store::open(path.clone()).unwrap();
        let harness = store.intern_public("dict_harness", "codex").unwrap();
        let repo = store.intern_public("dict_cwd", "/repo").unwrap();
        let other = store.intern_public("dict_cwd", "/other").unwrap();
        for (name, cwd_id) in [
            ("native-live", repo),
            ("native-dead", repo),
            ("native-other", other),
        ] {
            let session = store.intern_public("dict_session", name).unwrap();
            store
                .connection()
                .execute(
                    "INSERT INTO agent_session(session_id, harness_id, cwd_id) VALUES (?1, ?2, ?3)",
                    rusqlite::params![session, harness, cwd_id],
                )
                .unwrap();
        }
        store
            .record_status("native-live", 1, "live", None, None)
            .unwrap();
        store
            .record_status("native-dead", 1, "dead", None, None)
            .unwrap();
        store
            .record_status("native-other", 1, "live", None, None)
            .unwrap();
        for (lane, cwd, status) in [
            ("shell-live", "/repo", "live"),
            ("shell-dead", "/repo", "dead"),
            ("shell-other", "/other", "live"),
        ] {
            store
                .record_lane_spawn(&LaneSpawn {
                    lane: lane.into(),
                    cwd: Some(cwd.into()),
                    ts: 1,
                    ..LaneSpawn::default()
                })
                .unwrap();
            store.record_status(lane, 1, status, None, None).unwrap();
        }
        for (lane, key, created_ts) in [
            ("native-live", "event-native-live", 10),
            ("native-dead", "event-native-dead", 20),
            ("native-other", "event-native-other", 30),
            ("shell-live", "event-shell-live", 40),
            ("shell-dead", "event-shell-dead", 50),
            ("shell-other", "event-shell-other", 60),
        ] {
            store
                .record_trace_event(&TraceEvent {
                    event_key: key.into(),
                    lane: lane.into(),
                    trace: Some("trace-fixture".into()),
                    session: Some(lane.into()),
                    kind: "turn-finish".into(),
                    from_lane: Some("parent-lane".into()),
                    to_lane: Some(lane.into()),
                    started_ts: Some(7),
                    finished_ts: Some(8),
                    delivery_state: Some("delivered".into()),
                    classification: Some("completed".into()),
                    detail: "fixture detail".into(),
                    created_ts,
                })
                .unwrap();
        }

        let current = load_agent_session_graph(
            &store,
            AgentSessionGraphQuery {
                cwd: Some("/repo".into()),
                include_history: false,
                ..AgentSessionGraphQuery::default()
            },
        )
        .unwrap();
        assert_eq!(
            current
                .trace_events
                .iter()
                .map(|event| event.event_key.as_str())
                .collect::<Vec<_>>(),
            vec!["event-native-live", "event-shell-live"]
        );
        let json = serde_json::to_value(&current).unwrap();
        assert_eq!(
            json["trace_events"],
            serde_json::json!([
                {
                    "event_key": "event-native-live",
                    "lane": "native-live",
                    "trace": "trace-fixture",
                    "session": "native-live",
                    "kind": "turn-finish",
                    "from_lane": "parent-lane",
                    "to_lane": "native-live",
                    "started_ts": 7,
                    "finished_ts": 8,
                    "delivery_state": "delivered",
                    "classification": "completed",
                    "detail": "fixture detail",
                    "created_ts": 10
                },
                {
                    "event_key": "event-shell-live",
                    "lane": "shell-live",
                    "trace": "trace-fixture",
                    "session": "shell-live",
                    "kind": "turn-finish",
                    "from_lane": "parent-lane",
                    "to_lane": "shell-live",
                    "started_ts": 7,
                    "finished_ts": 8,
                    "delivery_state": "delivered",
                    "classification": "completed",
                    "detail": "fixture detail",
                    "created_ts": 40
                }
            ])
        );
        let event_keys = json["trace_events"][0]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            event_keys,
            vec![
                "event_key",
                "lane",
                "trace",
                "session",
                "kind",
                "from_lane",
                "to_lane",
                "started_ts",
                "finished_ts",
                "delivery_state",
                "classification",
                "detail",
                "created_ts",
            ]
        );

        let history = load_agent_session_graph(
            &store,
            AgentSessionGraphQuery {
                cwd: Some("/repo".into()),
                include_history: true,
                ..AgentSessionGraphQuery::default()
            },
        )
        .unwrap();
        assert_eq!(
            history
                .trace_events
                .iter()
                .map(|event| event.event_key.as_str())
                .collect::<Vec<_>>(),
            vec![
                "event-native-live",
                "event-native-dead",
                "event-shell-live",
                "event-shell-dead",
            ]
        );
        assert!(history
            .trace_events
            .iter()
            .all(|event| !event.lane.ends_with("other")));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn schema_version_one_legacy_graph_defaults_trace_events() {
        let graph: AgentSessionGraph =
            serde_json::from_str(r#"{"schema_version":1,"sessions":[],"edges":[],"shells":[]}"#)
                .unwrap();
        assert_eq!(graph.schema_version, AGENT_SESSION_GRAPH_SCHEMA_VERSION);
        assert!(graph.trace_events.is_empty());
    }

    #[test]
    fn tmux_session_window_and_pane_evidence_select_the_same_shell() {
        for target in ["sprefa-5", "sprefa-5:0.0", "sprefa-5:0.0.0", "%1205"] {
            let mut graph = AgentSessionGraph {
                schema_version: AGENT_SESSION_GRAPH_SCHEMA_VERSION,
                sessions: Vec::new(),
                edges: Vec::new(),
                shells: vec![AgentShellNode {
                    lane: "sprefa-coordinator".into(),
                    parent_lane: None,
                    harness: Some(HarnessId::Claude.as_str().to_owned()),
                    mode: None,
                    session_id: Some("da6da0ca-5ad6-4f2f-88f7-de82e79f1e6b".into()),
                    session: Some(AgentSessionIdentity {
                        harness: HarnessId::Claude.as_str().to_owned(),
                        id: "da6da0ca-5ad6-4f2f-88f7-de82e79f1e6b".into(),
                    }),
                    trace: None,
                    cwd: Some("/same-cwd".into()),
                    tmux: Some("sprefa-5:0.0".into()),
                    tmux_session: Some("sprefa-5".into()),
                    tmux_pane: Some("%1205".into()),
                    pid: Some(10),
                    state: "live".into(),
                    started_ts: None,
                    registered_at: None,
                }],
                trace_events: Vec::new(),
            };
            focus_graph(
                &mut graph,
                &AgentSessionGraphQuery {
                    tmux: Some(target.into()),
                    ..AgentSessionGraphQuery::default()
                },
            );
            assert_eq!(graph.shells.len(), 1, "target {target}");
            assert_eq!(graph.shells[0].lane, "sprefa-coordinator");
        }
    }

    #[test]
    fn focused_tmux_shell_serializes_its_rooted_family_without_cwd_inference() {
        let path =
            std::env::temp_dir().join(format!("boop-session-family-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let store = Store::open(path.clone()).unwrap();
        let codex = store.intern_public("dict_harness", "codex").unwrap();
        let claude = store.intern_public("dict_harness", "claude").unwrap();
        for (id, harness, started) in [
            ("codex-coordinator-root", codex, 10),
            ("dispatched-child", codex, 20),
            ("native-subagent", codex, 30),
            ("completed-descendant", codex, 40),
            ("claude-coordinator-root", claude, 50),
        ] {
            let session = store.intern_public("dict_session", id).unwrap();
            store.connection().execute(
                "INSERT INTO agent_session(session_id, harness_id, started_ts) VALUES (?1, ?2, ?3)",
                rusqlite::params![session, harness, started],
            ).unwrap();
        }
        store
            .record_status(
                "codex-coordinator-root",
                60,
                "live",
                Some(7),
                Some("%focused"),
            )
            .unwrap();
        store
            .record_status("dispatched-child", 70, "live", Some(8), Some("%child"))
            .unwrap();
        store
            .record_status("native-subagent", 80, "live", None, None)
            .unwrap();
        store
            .record_status("completed-descendant", 90, "dead", None, None)
            .unwrap();
        store
            .record_status(
                "claude-coordinator-root",
                100,
                "live",
                Some(9),
                Some("%unrelated"),
            )
            .unwrap();
        store
            .add_edge_at("codex-coordinator-root", "dispatched-child", "spawned", 21)
            .unwrap();
        store
            .add_edge_at("dispatched-child", "native-subagent", "spawned", 31)
            .unwrap();
        store
            .add_edge_at("native-subagent", "completed-descendant", "spawned", 41)
            .unwrap();
        for id in [
            "codex-coordinator-root",
            "dispatched-child",
            "native-subagent",
            "completed-descendant",
        ] {
            store
                .attach_trace(id, "trace-focused", "fixture", 61)
                .unwrap();
        }
        store
            .attach_trace("claude-coordinator-root", "trace-unrelated", "fixture", 101)
            .unwrap();
        let mut routes = BTreeMap::new();
        for (lane, session_id, tmux, parent) in [
            (
                "codex-coordinator",
                "codex-coordinator-root",
                "%focused",
                None,
            ),
            (
                "dispatch-lane",
                "dispatched-child",
                "%child",
                Some("codex-coordinator"),
            ),
            (
                "claude-coordinator",
                "claude-coordinator-root",
                "%unrelated",
                None,
            ),
        ] {
            routes.insert(
                lane.into(),
                Route {
                    kind: "coordinator".into(),
                    harness: HarnessId::parse(lane.split('-').next().unwrap_or_default())
                        .or(Some(HarnessId::Codex)),
                    tmux: Some(tmux.into()),
                    cwd: Some("/same-cwd-for-all".into()),
                    model: None,
                    mode: Some("interactive".into()),
                    session_id: Some(session_id.into()),
                    source_path: None,
                    parent: parent.map(str::to_owned),
                    goal: None,
                    registered_at: Some("2026-08-18T00:00:00Z".into()),
                    base_sha: None,
                    worktree_dir: None,
                    app_server_socket: None,
                },
            );
        }
        let mux = FakeMux::available(&[]);
        let processes = SysinfoSnapshot::capture().unwrap();
        let graph = load_agent_session_graph_with_runtime(
            &store,
            AgentSessionGraphQuery {
                tmux: Some("%focused".into()),
                include_history: true,
                ..AgentSessionGraphQuery::default()
            },
            AgentSessionGraphRuntime {
                routes: &routes,
                messages: &[],
                multiplexer: &mux,
                tmux_socket: None,
                processes: &processes,
            },
        )
        .unwrap();
        let receipt = serde_json::to_value(&graph).unwrap();
        assert_eq!(
            receipt["shells"]
                .as_array()
                .unwrap()
                .iter()
                .map(|shell| shell["lane"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["codex-coordinator", "dispatch-lane"]
        );
        assert_eq!(
            receipt["sessions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|node| node["session"]["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec![
                "codex-coordinator-root",
                "completed-descendant",
                "dispatched-child",
                "native-subagent"
            ]
        );
        assert_eq!(receipt["sessions"][0]["trace"], "trace-focused");
        assert_eq!(receipt["sessions"][1]["finished_ts"], 90);
        assert_eq!(receipt["edges"][0]["first_ts"], 21);
        assert_eq!(
            receipt["shells"][0]["session"],
            serde_json::json!({"harness":"codex","id":"codex-coordinator-root"})
        );
        assert_eq!(receipt["shells"][0]["tmux_pane"], "%focused");
        assert!(!receipt.to_string().contains("claude-coordinator-root"));
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}
