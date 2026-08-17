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
    pub last_activity_ts: Option<u64>,
}

/// One native parent-child session relation.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct AgentSessionEdge {
    pub parent: AgentSessionIdentity,
    pub child: AgentSessionIdentity,
    pub kind: String,
}

/// One registered lane with no harness transcript session.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct AgentShellNode {
    pub lane: String,
    pub parent_lane: Option<String>,
    pub harness: Option<String>,
    pub mode: Option<String>,
    pub session_id: Option<String>,
    pub cwd: Option<PathBuf>,
    pub tmux: Option<String>,
    pub pid: Option<u32>,
    pub state: String,
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
           st.value AS state
      FROM agent_session a
      JOIN dict_session s ON s.id = a.session_id
      JOIN dict_harness h ON h.id = a.harness_id
      LEFT JOIN dict_cwd c ON c.id = a.cwd_id
      LEFT JOIN agent_live live ON live.session_id = a.session_id
      LEFT JOIN dict_pane p ON p.id = live.tmux_pane_id
      LEFT JOIN dict_status st ON st.id = live.status_id
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
       END AS last_ts
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
        .cwd
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned());
    let history = query.include_history;
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
            last_activity_ts: row
                .get::<_, Option<i64>>(5)?
                .and_then(|value| u64::try_from(value).ok()),
        })
    })?;
    let sessions = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let included = sessions
        .iter()
        .map(|session| session.session.id.as_str())
        .collect::<BTreeSet<_>>();
    let edge_sql = "SELECT p.value, c.value, hp.value, hc.value, k.value
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
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|edge| {
            included.contains(edge.parent.id.as_str()) && included.contains(edge.child.id.as_str())
        })
        .collect::<Vec<_>>();

    let shell_sql = "SELECT lane.value, parent.value, cwd.value, pane.value,
                            live.pid, COALESCE(status.value, 'unknown')
                       FROM agent_lane lane_row
                       JOIN dict_session lane ON lane.id = lane_row.lane_id
                       LEFT JOIN dict_session parent ON parent.id = lane_row.parent_lane_id
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
            cwd: row.get::<_, Option<String>>(2)?.map(PathBuf::from),
            tmux: row.get(3)?,
            pid: row
                .get::<_, Option<i64>>(4)?
                .and_then(|pid| u32::try_from(pid).ok()),
            state: row.get(5)?,
        })
    })? {
        let shell = row?;
        if !included.contains(shell.lane.as_str()) && seen_lanes.insert(shell.lane.clone()) {
            shells.push(shell);
        }
    }

    let trace_events = query_trace_events(store, &sessions, &shells)?;

    Ok(AgentSessionGraph {
        schema_version: AGENT_SESSION_GRAPH_SCHEMA_VERSION,
        sessions,
        edges,
        shells,
        trace_events,
    })
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
    let include_history = query.include_history;
    let cwd = query
        .cwd
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned());
    let mut graph = load_agent_session_graph(store, query)?;
    let session_keys = graph
        .sessions
        .iter()
        .map(|session| session_key(&session.session))
        .collect::<BTreeSet<_>>();
    let rows = runtime_snapshot(RuntimeSnapshotInput {
        store,
        routes: runtime.routes,
        messages: runtime.messages,
        multiplexer: runtime.multiplexer,
        tmux_socket: runtime.tmux_socket,
        processes: runtime.processes,
    })?;
    for row in rows {
        if runtime_row_is_native(&row, &session_keys) {
            continue;
        }
        if let Some(shell) = shell_from_runtime(row) {
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
    graph.trace_events = query_trace_events(store, &graph.sessions, &graph.shells)?;
    Ok(graph)
}

fn shell_from_runtime(row: AgentRuntimeRow) -> Option<AgentShellNode> {
    let route = row.route?;
    if route.kind != "shell" && route.harness.is_some() {
        return None;
    }
    let tmux = row.tmux_target.or(row.tmux_pane)?;
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
        harness: route.harness.clone(),
        mode: route.mode.clone(),
        session_id: route.session_id,
        cwd: row.cwd.map(PathBuf::from),
        tmux: Some(tmux),
        pid: row.pid.and_then(|pid| u32::try_from(pid).ok()),
        state: state.to_owned(),
    })
}

fn session_key(identity: &AgentSessionIdentity) -> String {
    format!("{}\0{}", identity.harness, identity.id)
}

fn runtime_row_is_native(row: &AgentRuntimeRow, session_keys: &BTreeSet<String>) -> bool {
    let Some(session) = row.session.as_deref() else {
        return false;
    };
    let Some(route) = row.route.as_ref() else {
        return false;
    };
    route
        .harness
        .as_ref()
        .map(|harness| session_keys.contains(&format!("{harness}\0{session}")))
        .unwrap_or(false)
}

#[cfg(test)]
pub(crate) fn assert_fixture_sessions_project(
    adapter: &dyn crate::harness::Harness,
    sessions: &[crate::harness::SessionRef],
    expected_edges: usize,
) {
    let path = std::env::temp_dir().join(format!(
        "boop-session-graph-fixture-{}-{}.db",
        std::process::id(),
        adapter.id()
    ));
    let _ = std::fs::remove_file(&path);
    let store = Store::open(path.clone()).unwrap();
    for session in sessions {
        crate::ident::sync_session(&store, adapter, session).unwrap();
    }
    let graph = load_agent_session_graph(
        &store,
        AgentSessionGraphQuery {
            cwd: None,
            include_history: true,
        },
    )
    .unwrap();
    assert_eq!(graph.sessions.len(), sessions.len());
    assert!(graph.edges.len() >= expected_edges);
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ident::{LaneSpawn, TraceEvent};
    use crate::runtime::{ProcessLiveness, ResolvedRoute, RuntimeLiveness, TmuxLiveness};

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
    fn only_shell_routes_project_as_shell_nodes() {
        let route = ResolvedRoute {
            lane: "lane".into(),
            kind: "lane".into(),
            harness: Some("codex".into()),
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
        assert!(shell_from_runtime(native).is_none());
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
            .any(|session| session.session.harness == "claude"));
        assert!(graph
            .sessions
            .iter()
            .any(|session| session.session.harness == "opencode"));
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
}
