//! Typed projection of native harness sessions and shell-only lanes.

use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::Result;
use serde::Serialize;

use crate::bus::{Message, Route};
use crate::proc::ProcReader;
use crate::runtime::{runtime_snapshot, AgentRuntimeRow, RuntimeSnapshotInput};
use crate::tmux::Multiplexer;
use crate::Store;

/// Version of the JSON session-graph document.
pub const AGENT_SESSION_GRAPH_SCHEMA_VERSION: u32 = 1;

/// Filters for one session-graph read.
#[derive(Clone, Debug, Default)]
pub struct AgentSessionGraphQuery {
    pub cwd: Option<PathBuf>,
    pub include_history: bool,
}

/// Function type for the pure durable graph projection.
pub type LoadAgentSessionGraph = fn(&Store, AgentSessionGraphQuery) -> Result<AgentSessionGraph>;

/// The complete native-session and shell projection.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AgentSessionGraph {
    pub schema_version: u32,
    pub sessions: Vec<AgentSessionNode>,
    pub edges: Vec<AgentSessionEdge>,
    pub shells: Vec<AgentShellNode>,
}

/// One normalized harness transcript session.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AgentSessionNode {
    pub session: String,
    pub harness: String,
    pub cwd: Option<PathBuf>,
    pub tmux: Option<String>,
    pub state: Option<String>,
    pub last_activity_ts: Option<u64>,
}

/// One native parent-child session relation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AgentSessionEdge {
    pub parent: String,
    pub child: String,
    pub kind: String,
}

/// One registered lane with no harness transcript session.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AgentShellNode {
    pub lane: String,
    pub parent_lane: Option<String>,
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
    let sql = "SELECT s.value, h.value, c.value, p.value, st.value,
                      CASE WHEN turns.last_ts IS NULL AND usage.last_ts IS NULL
                           THEN NULL
                           ELSE MAX(COALESCE(turns.last_ts, 0), COALESCE(usage.last_ts, 0))
                      END AS last_ts
                 FROM agent_session a
                 JOIN dict_session s ON s.id = a.session_id
                 JOIN dict_harness h ON h.id = a.harness_id
                 LEFT JOIN dict_cwd c ON c.id = a.cwd_id
                 LEFT JOIN agent_live live ON live.session_id = a.session_id
                 LEFT JOIN dict_pane p ON p.id = live.tmux_pane_id
                 LEFT JOIN dict_status st ON st.id = live.status_id
                 LEFT JOIN (SELECT session_id, MAX(ts) AS last_ts
                              FROM agent_turn GROUP BY session_id) turns
                   ON turns.session_id = a.session_id
                 LEFT JOIN (SELECT session_id, MAX(ts) AS last_ts
                              FROM agent_usage GROUP BY session_id) usage
                   ON usage.session_id = a.session_id
                WHERE (?1 IS NULL OR c.value = ?1)
                  AND (?2 OR st.value IS NULL OR st.value = 'live')
                ORDER BY s.value";
    let mut statement = store.connection().prepare(sql)?;
    let rows = statement.query_map(rusqlite::params![cwd, history], |row| {
        Ok(AgentSessionNode {
            session: row.get(0)?,
            harness: row.get(1)?,
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
        .map(|session| session.session.as_str())
        .collect::<BTreeSet<_>>();

    let edge_sql = "SELECT p.value, c.value, k.value
                      FROM agent_edge e
                      JOIN dict_session p ON p.id = e.parent_session_id
                      JOIN dict_session c ON c.id = e.child_session_id
                      JOIN dict_edekind k ON k.id = e.edge_kind_id
                     ORDER BY p.value, c.value, k.value";
    let mut statement = store.connection().prepare(edge_sql)?;
    let edges = statement
        .query_map([], |row| {
            Ok(AgentSessionEdge {
                parent: row.get(0)?,
                child: row.get(1)?,
                kind: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|edge| {
            included.contains(edge.parent.as_str()) && included.contains(edge.child.as_str())
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
                        AND (?2 OR COALESCE(status.value, 'unknown') = 'live')
                      ORDER BY lane.value, lane_row.spawned_ts DESC";
    let mut statement = store.connection().prepare(shell_sql)?;
    let mut shells = Vec::new();
    let mut seen_lanes = BTreeSet::new();
    for row in statement.query_map(rusqlite::params![cwd, history], |row| {
        Ok(AgentShellNode {
            lane: row.get(0)?,
            parent_lane: row.get(1)?,
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

    Ok(AgentSessionGraph {
        schema_version: AGENT_SESSION_GRAPH_SCHEMA_VERSION,
        sessions,
        edges,
        shells,
    })
}

/// Load the durable graph and merge one bounded tmux/process observation.
pub fn load_agent_session_graph_with_runtime(
    store: &Store,
    query: AgentSessionGraphQuery,
    runtime: AgentSessionGraphRuntime<'_>,
) -> Result<AgentSessionGraph> {
    let include_history = query.include_history;
    let mut graph = load_agent_session_graph(store, query)?;
    let session_ids = graph
        .sessions
        .iter()
        .map(|session| session.session.as_str())
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
        if session_ids.contains(row.session.as_deref().unwrap_or("")) {
            continue;
        }
        if let Some(shell) = shell_from_runtime(row) {
            if !include_history && shell.state != "live" {
                continue;
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
    Ok(graph)
}

fn shell_from_runtime(row: AgentRuntimeRow) -> Option<AgentShellNode> {
    let route = row.route?;
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
        cwd: row.cwd.map(PathBuf::from),
        tmux: Some(tmux),
        pid: row.pid.and_then(|pid| u32::try_from(pid).ok()),
        state: state.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ident::LaneSpawn;

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
    fn all_harness_fixture_rows_use_the_same_native_edge_shape() {
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
        let _ = std::fs::remove_file(path);
    }
}
