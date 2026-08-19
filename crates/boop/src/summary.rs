//! Versioned CASS-compatible agent summary.
//!
//! This contract deliberately contains Boop-owned agent, runtime, mailbox,
//! transcript, usage, and completion facts only. CASS issue, reservation, and
//! provider records are separate contracts and do not appear in these types.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use serde::Serialize;

use crate::activity::ToolResultAvailability;
use crate::bus::{Message, Route};
use crate::proc::{ProcReader, SysinfoSnapshot};
use crate::runtime::{runtime_snapshot, AgentRuntimeRow, RuntimeSnapshotInput};
use crate::tmux::Multiplexer;
use crate::Store;

/// Schema version emitted by [`AgentSummary`].
pub const AGENT_SUMMARY_SCHEMA_VERSION: u32 = 1;

/// Inputs for one agent-summary query. Process and tmux acquisition are owned
/// by the caller, so one stable observation can be joined across every lane.
pub struct AgentSummaryQuery<'a> {
    pub store: &'a Store,
    pub routes: &'a BTreeMap<String, Route>,
    pub messages: &'a [Message],
    pub multiplexer: &'a dyn Multiplexer,
    pub tmux_socket: Option<&'a str>,
    pub processes: &'a dyn ProcReader,
}

/// Stable agent summary consumed by CASS-compatible views.
///
/// CASS issue, reservation, and provider fields are intentionally absent.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AgentSummary {
    pub schema_version: u32,
    pub active_agents: u64,
    pub agents: Vec<AgentSummaryAgent>,
}

/// One lane's combined runtime and transcript activity row.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AgentSummaryAgent {
    /// The complete bounded runtime row, including route, process, worktree,
    /// completion, and diagnostic facts.
    pub runtime: AgentRuntimeRow,
    pub activity: AgentSummaryActivity,
}

/// Transcript and usage counts joined through the runtime-selected trace.
/// Ambiguous runtime traces receive this type's zero default instead of a
/// combined historical lane total.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AgentSummaryActivity {
    pub user: u64,
    pub assistant: u64,
    pub tool_call: u64,
    pub total: u64,
    pub calls: u64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_create_5m_tokens: i64,
    pub cache_create_1h_tokens: i64,
    pub cache_read_tokens: i64,
    pub first_activity_ts: Option<i64>,
    pub last_activity_ts: Option<i64>,
    pub tool_result_availability: ToolResultAvailability,
}

impl Default for AgentSummaryActivity {
    fn default() -> Self {
        Self {
            user: 0,
            assistant: 0,
            tool_call: 0,
            total: 0,
            calls: 0,
            input_tokens: 0,
            output_tokens: 0,
            cache_create_5m_tokens: 0,
            cache_create_1h_tokens: 0,
            cache_read_tokens: 0,
            first_activity_ts: None,
            last_activity_ts: None,
            tool_result_availability: ToolResultAvailability::Unavailable,
        }
    }
}

/// Join lane activity counts to a single bounded runtime observation.
pub fn agent_summary(query: AgentSummaryQuery<'_>) -> Result<AgentSummary> {
    let runtime = runtime_snapshot(RuntimeSnapshotInput {
        store: query.store,
        routes: query.routes,
        messages: query.messages,
        multiplexer: query.multiplexer,
        tmux_socket: query.tmux_socket,
        processes: query.processes,
    })?;
    let selected_traces = runtime
        .iter()
        .filter_map(|row| row.trace.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let activity = query
        .store
        .activity_counts_for_traces(&selected_traces)?
        .into_iter()
        .map(|row| {
            (
                row.identity,
                AgentSummaryActivity {
                    user: row.user,
                    assistant: row.assistant,
                    tool_call: row.tool_call,
                    total: row.total,
                    calls: row.calls,
                    input_tokens: row.input_tokens,
                    output_tokens: row.output_tokens,
                    cache_create_5m_tokens: row.cache_create_5m_tokens,
                    cache_create_1h_tokens: row.cache_create_1h_tokens,
                    cache_read_tokens: row.cache_read_tokens,
                    first_activity_ts: row.first_activity_ts,
                    last_activity_ts: row.last_activity_ts,
                    tool_result_availability: row.tool_result_availability,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut agents = runtime
        .into_iter()
        .map(|row| AgentSummaryAgent {
            activity: row
                .trace
                .as_ref()
                .and_then(|trace| activity.get(trace))
                .cloned()
                .unwrap_or_default(),
            runtime: row,
        })
        .collect::<Vec<_>>();
    agents.sort_by(|left, right| left.runtime.lane.cmp(&right.runtime.lane));
    let active_agents = agents
        .iter()
        .filter(|agent| {
            matches!(
                agent.runtime.liveness.tmux,
                crate::runtime::TmuxLiveness::Live
            ) || matches!(
                agent.runtime.liveness.process,
                crate::runtime::ProcessLiveness::Live
            )
        })
        .count() as u64;
    Ok(AgentSummary {
        schema_version: AGENT_SUMMARY_SCHEMA_VERSION,
        active_agents,
        agents,
    })
}

/// Production summary acquisition: capture the process table once, list tmux
/// sessions once, then join both observations across all lanes.
pub fn agent_summary_now(
    store: &Store,
    routes: &BTreeMap<String, Route>,
    messages: &[Message],
) -> Result<AgentSummary> {
    let processes = SysinfoSnapshot::capture()?;
    agent_summary(AgentSummaryQuery {
        store,
        routes,
        messages,
        multiplexer: crate::tmux::mux(),
        tmux_socket: None,
        processes: &processes,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;

    use crate::bus::{Message, Route};
    use crate::ident::LaneSpawn;
    use crate::proc::{ProcReader, ProcessInfo};
    use crate::test_support::FakeMux;
    use crate::Store;

    use super::{agent_summary, AgentSummaryQuery, AGENT_SUMMARY_SCHEMA_VERSION};

    #[derive(Default)]
    struct FixedProcesses {
        rows: HashMap<u32, ProcessInfo>,
    }

    impl ProcReader for FixedProcesses {
        fn is_alive(&self, pid: u32) -> bool {
            self.rows.contains_key(&pid)
        }

        fn process(&self, pid: u32) -> Option<ProcessInfo> {
            self.rows.get(&pid).cloned()
        }

        fn children(&self, _: u32) -> Vec<u32> {
            Vec::new()
        }

        fn descendants(&self, _: u32) -> Vec<u32> {
            Vec::new()
        }

        fn descendant_count(&self, _: u32) -> usize {
            0
        }
    }

    fn fresh_store(name: &str) -> (PathBuf, Store) {
        let path = std::env::temp_dir().join(format!(
            "boop-summary-{name}-{}-{}.db",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let _ = std::fs::remove_file(&path);
        (path.clone(), Store::open(path).unwrap())
    }

    #[test]
    fn schema_fixture_joins_activity_runtime_mailbox_and_completion() {
        let (path, store) = fresh_store("schema");
        store
            .record_lane_spawn(&LaneSpawn {
                lane: "lane-a".into(),
                trace: Some("trace-a".into()),
                ts: 10,
                ..LaneSpawn::default()
            })
            .unwrap();
        let session = store.intern_public("dict_session", "session-a").unwrap();
        let harness = store.intern_public("dict_harness", "codex").unwrap();
        let role = store.intern_public("dict_role", "assistant").unwrap();
        store
            .connection()
            .execute(
                "INSERT INTO agent_session(session_id, harness_id, nickname, started_ts) VALUES (?1, ?2, 'a', 11)",
                rusqlite::params![session, harness],
            )
            .unwrap();
        store
            .connection()
            .execute(
                "INSERT INTO agent_turn(session_id, turn, ts, role_id, said) VALUES (?1, 1, 12, ?2, '')",
                rusqlite::params![session, role],
            )
            .unwrap();
        store
            .attach_trace("session-a", "trace-a", "fixture", 11)
            .unwrap();
        store
            .record_status("session-a", 13, "live", Some(101), Some("%1"))
            .unwrap();
        let mut routes = BTreeMap::new();
        routes.insert(
            "lane-a".into(),
            Route {
                kind: "lane".into(),
                harness: Some("codex".into()),
                tmux: Some("lane-a:0".into()),
                cwd: None,
                model: None,
                mode: None,
                session_id: Some("session-a".into()),
                source_path: None,
                parent: None,
                goal: None,
                registered_at: None,
                base_sha: None,
                worktree_dir: None,
            },
        );
        let messages = vec![Message {
            id: "done".into(),
            from: "lane-a".into(),
            to: "parent".into(),
            from_timestamp: "2".into(),
            to_timestamp: None,
            kind: "result".into(),
            reply_to: None,
            body: "lane lane-a done rc=0".into(),
            r#ref: None,
            rc: Some(0),
            detail: None,
        }];
        let mux = FakeMux::available(&["lane-a"]);
        let processes = FixedProcesses::default();
        let summary = agent_summary(AgentSummaryQuery {
            store: &store,
            routes: &routes,
            messages: &messages,
            multiplexer: &mux,
            tmux_socket: None,
            processes: &processes,
        })
        .unwrap();
        assert_eq!(summary.schema_version, AGENT_SUMMARY_SCHEMA_VERSION);
        assert_eq!(summary.active_agents, 1);
        assert_eq!(
            serde_json::to_string_pretty(&summary).unwrap(),
            r#"{
  "schema_version": 1,
  "active_agents": 1,
  "agents": [
    {
      "runtime": {
        "lane": "lane-a",
        "trace": "trace-a",
        "root_session": "session-a",
        "session": "session-a",
        "parent": null,
        "route": {
          "lane": "lane-a",
          "kind": "lane",
          "harness": "codex",
          "tmux": "lane-a:0",
          "cwd": null,
          "model": null,
          "mode": null,
          "session_id": "session-a",
          "source_path": null,
          "parent": null,
          "goal": null,
          "registered_at": null
        },
        "cwd": null,
        "tmux_target": "lane-a:0",
        "tmux_pane": "%1",
        "pid": 101,
        "reported_status": "live",
        "liveness": {
          "tmux": "live",
          "process": "dead"
        },
        "completion": {
          "id": "done",
          "from": "lane-a",
          "to": "parent",
          "timestamp": "2",
          "body": "lane lane-a done rc=0",
          "exit_code": 0
        },
        "mailbox": {
          "inbox": 0,
          "outbox": 1,
          "unacknowledged": 0
        },
        "worktree": {
          "route_cwd": null,
          "process_cwd": null
        },
        "diagnostics": []
      },
      "activity": {
        "user": 0,
        "assistant": 1,
        "tool_call": 0,
        "total": 1,
        "calls": 0,
        "input_tokens": 0,
        "output_tokens": 0,
        "cache_create_5m_tokens": 0,
        "cache_create_1h_tokens": 0,
        "cache_read_tokens": 0,
        "first_activity_ts": 12,
        "last_activity_ts": 12,
        "tool_result_availability": "unavailable"
      }
    }
  ]
}"#
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn multiple_lanes_share_one_tmux_observation_and_process_snapshot() {
        let (path, store) = fresh_store("bounded");
        let mut routes = BTreeMap::new();
        for number in 0..3 {
            let lane = format!("lane-{number:03}");
            store
                .record_lane_spawn(&LaneSpawn {
                    lane: lane.clone(),
                    trace: Some(format!("trace-{number:03}")),
                    ts: number,
                    ..LaneSpawn::default()
                })
                .unwrap();
            routes.insert(
                lane,
                Route {
                    kind: "shell".into(),
                    harness: None,
                    tmux: Some(format!("shell-{number:03}")),
                    cwd: None,
                    model: None,
                    mode: None,
                    session_id: None,
                    source_path: None,
                    parent: None,
                    goal: None,
                    registered_at: None,
                    base_sha: None,
                    worktree_dir: None,
                },
            );
        }
        let mux = FakeMux::available(&[]);
        let processes = FixedProcesses::default();
        let summary = agent_summary(AgentSummaryQuery {
            store: &store,
            routes: &routes,
            messages: &[],
            multiplexer: &mux,
            tmux_socket: None,
            processes: &processes,
        })
        .unwrap();
        assert_eq!(summary.agents.len(), 3);
        assert_eq!(mux.observations.load(Ordering::SeqCst), 1);
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reused_lane_with_ambiguous_traces_has_non_combined_activity() {
        let (path, store) = fresh_store("reused-lane");
        for (trace, session, ts) in [
            ("trace-old", "session-old", 10),
            ("trace-new", "session-new", 20),
        ] {
            store
                .record_lane_spawn(&LaneSpawn {
                    lane: "lane-a".into(),
                    trace: Some(trace.into()),
                    ts,
                    ..LaneSpawn::default()
                })
                .unwrap();
            store.attach_trace(session, trace, "fixture", ts).unwrap();
            let session_id = store.intern_public("dict_session", session).unwrap();
            let role_id = store.intern_public("dict_role", "assistant").unwrap();
            store
                .connection()
                .execute(
                    "INSERT INTO agent_turn(session_id, turn, ts, role_id, said) VALUES (?1, 1, ?2, ?3, '')",
                    rusqlite::params![session_id, ts as i64, role_id],
                )
                .unwrap();
        }
        let mux = FakeMux::available(&[]);
        let processes = FixedProcesses::default();
        let summary = agent_summary(AgentSummaryQuery {
            store: &store,
            routes: &BTreeMap::new(),
            messages: &[],
            multiplexer: &mux,
            tmux_socket: None,
            processes: &processes,
        })
        .unwrap();
        let agent = &summary.agents[0];
        assert!(agent.runtime.trace.is_none());
        assert!(agent.runtime.diagnostics.iter().any(|diagnostic| matches!(
            diagnostic,
            crate::RuntimeDiagnostic::AmbiguousTrace { .. }
        )));
        assert_eq!(agent.activity.total, 0);
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}
