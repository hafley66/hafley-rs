//! Typed resolution of one lane's runtime identity.
//!
//! A lane name is a durable placeholder. Harness session ids are process
//! coordinates and can change on resume, compaction, or replacement. This
//! module is the one read seam that joins the placeholder to its trace,
//! attached harness sessions, route, observed process, and completion mail.
//! Callers receive diagnostics for incomplete or ambiguous evidence instead of
//! repeating dictionary-table joins.

use std::collections::BTreeMap;

use anyhow::Result;
use rusqlite::OptionalExtension;
use serde::Serialize;

use crate::bus::{Message, Route};
use crate::ident::Store;

/// A typed reason why one part of a lane runtime could not be resolved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeDiagnostic {
    MissingLane,
    MissingTrace {
        lane: String,
    },
    AmbiguousTrace {
        lane: String,
        traces: Vec<String>,
    },
    MissingCurrentSession {
        trace: String,
    },
    AmbiguousCurrentSession {
        trace: String,
        sessions: Vec<String>,
        activity_ts: i64,
    },
    MissingRoute,
    AmbiguousRoute {
        lanes: Vec<String>,
    },
    MissingCompletion,
}

/// The route registry data for the lane, copied into a typed read result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResolvedRoute {
    pub lane: String,
    pub kind: String,
    pub harness: Option<String>,
    pub tmux: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub mode: Option<String>,
    pub session_id: Option<String>,
    pub source_path: Option<String>,
    pub parent: Option<String>,
    pub goal: Option<String>,
    pub registered_at: Option<String>,
}

impl ResolvedRoute {
    fn from_route(lane: &str, route: &Route) -> Self {
        ResolvedRoute {
            lane: lane.to_owned(),
            kind: route.kind.clone(),
            harness: route.harness.clone(),
            tmux: route.tmux.clone(),
            cwd: route.cwd.clone(),
            model: route.model.clone(),
            mode: route.mode.clone(),
            session_id: route.session_id.clone(),
            source_path: route.source_path.clone(),
            parent: route.parent.clone(),
            goal: route.goal.clone(),
            registered_at: route.registered_at.clone(),
        }
    }
}

/// Process evidence attached to the selected harness session.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProcessIdentity {
    pub session: String,
    pub status: Option<String>,
    pub pid: Option<i64>,
    pub tmux_pane: Option<String>,
    pub alive: Option<bool>,
}

/// A completion row from the lane mailbox.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CompletionRecord {
    pub id: String,
    pub from: String,
    pub to: String,
    pub timestamp: String,
    pub body: String,
    pub exit_code: Option<i32>,
}

/// The complete lane-to-runtime projection.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LaneRuntime {
    pub lane: String,
    pub trace: Option<String>,
    pub root_session: Option<String>,
    pub current_session: Option<String>,
    pub route: Option<ResolvedRoute>,
    pub process: Option<ProcessIdentity>,
    pub completion: Option<CompletionRecord>,
    /// The placeholder row is deliberately separate from generated harness
    /// sessions. This distinction survives session replacement.
    pub placeholder_session: Option<String>,
    pub generated_sessions: Vec<String>,
    pub diagnostics: Vec<RuntimeDiagnostic>,
}

#[derive(Clone, Debug)]
struct AttachedSession {
    session: String,
    activity_ts: i64,
    generated: bool,
}

/// Resolve all runtime facets for `lane`, using only trace attachments as the
/// session boundary and transcript/usage timestamps as activity evidence.
pub fn resolve(
    store: &Store,
    lane: &str,
    routes: &BTreeMap<String, Route>,
    messages: &[Message],
) -> Result<LaneRuntime> {
    let mut runtime = LaneRuntime {
        lane: lane.to_owned(),
        trace: None,
        root_session: None,
        current_session: None,
        route: None,
        process: None,
        completion: latest_completion(messages, lane),
        placeholder_session: None,
        generated_sessions: Vec::new(),
        diagnostics: Vec::new(),
    };

    if runtime.completion.is_none() {
        runtime
            .diagnostics
            .push(RuntimeDiagnostic::MissingCompletion);
    }

    let trace_names = store.runtime_trace_names(lane)?;
    if trace_names.is_empty() {
        runtime
            .diagnostics
            .push(if store.runtime_lane_exists(lane)? {
                RuntimeDiagnostic::MissingTrace {
                    lane: lane.to_owned(),
                }
            } else {
                RuntimeDiagnostic::MissingLane
            });
    } else if trace_names.len() > 1 {
        runtime.diagnostics.push(RuntimeDiagnostic::AmbiguousTrace {
            lane: lane.to_owned(),
            traces: trace_names,
        });
    } else {
        let trace = trace_names[0].clone();
        runtime.trace = Some(trace.clone());
        runtime.root_session = store.runtime_trace_root(&trace)?;
        let attached = store.runtime_attached_sessions(&trace)?;
        runtime.placeholder_session = attached
            .iter()
            .find(|session| session.session == lane)
            .map(|session| session.session.clone())
            .or_else(|| Some(lane.to_owned()));
        runtime.generated_sessions = attached
            .iter()
            .filter(|session| session.generated && session.session != lane)
            .map(|session| session.session.clone())
            .collect();

        let candidates: Vec<&AttachedSession> = attached
            .iter()
            .filter(|session| session.generated && session.session != lane)
            .collect();
        if candidates.is_empty() {
            runtime
                .diagnostics
                .push(RuntimeDiagnostic::MissingCurrentSession { trace });
        } else {
            let activity_ts = candidates
                .iter()
                .map(|session| session.activity_ts)
                .max()
                .unwrap_or(0);
            let current: Vec<&AttachedSession> = candidates
                .into_iter()
                .filter(|session| session.activity_ts == activity_ts)
                .collect();
            if current.len() > 1 {
                runtime
                    .diagnostics
                    .push(RuntimeDiagnostic::AmbiguousCurrentSession {
                        trace,
                        sessions: current
                            .iter()
                            .map(|session| session.session.clone())
                            .collect(),
                        activity_ts,
                    });
            } else if let Some(session) = current.first() {
                runtime.current_session = Some(session.session.clone());
            }
        }
    }

    let route_matches = route_matches(routes, lane, runtime.current_session.as_deref());
    match route_matches.as_slice() {
        [] => runtime.diagnostics.push(RuntimeDiagnostic::MissingRoute),
        [(route_lane, route)] => {
            runtime.route = Some(ResolvedRoute::from_route(route_lane, route));
        }
        matches => runtime.diagnostics.push(RuntimeDiagnostic::AmbiguousRoute {
            lanes: matches
                .iter()
                .map(|(route_lane, _)| (*route_lane).clone())
                .collect(),
        }),
    }

    let process_session = runtime
        .current_session
        .as_deref()
        .or(runtime.placeholder_session.as_deref());
    if let Some(session) = process_session {
        runtime.process = store.runtime_process(session)?;
    }
    if runtime.process.is_none() {
        if let Some(route) = runtime.route.as_ref() {
            if route.tmux.is_some() {
                runtime.process = Some(ProcessIdentity {
                    session: runtime
                        .current_session
                        .clone()
                        .unwrap_or_else(|| lane.to_owned()),
                    status: None,
                    pid: None,
                    tmux_pane: route.tmux.clone(),
                    alive: None,
                });
            }
        }
    }
    Ok(runtime)
}

fn route_matches<'a>(
    routes: &'a BTreeMap<String, Route>,
    lane: &str,
    current_session: Option<&str>,
) -> Vec<(&'a String, &'a Route)> {
    if let Some(route) = routes.get(lane) {
        return routes
            .iter()
            .find(|(name, _)| name.as_str() == lane)
            .map(|(name, _)| vec![(name, route)])
            .unwrap_or_default();
    }
    routes
        .iter()
        .filter(|(_, route)| {
            current_session.is_some_and(|session| route.session_id.as_deref() == Some(session))
        })
        .collect()
}

fn latest_completion(messages: &[Message], lane: &str) -> Option<CompletionRecord> {
    messages
        .iter()
        .filter(|message| message.kind == "result" && (message.from == lane || message.to == lane))
        .max_by(|left, right| {
            left.from_timestamp
                .cmp(&right.from_timestamp)
                .then_with(|| left.id.cmp(&right.id))
        })
        .map(|message| CompletionRecord {
            id: message.id.clone(),
            from: message.from.clone(),
            to: message.to.clone(),
            timestamp: message.from_timestamp.clone(),
            body: message.body.clone(),
            exit_code: parse_exit_code(&message.body),
        })
}

fn parse_exit_code(body: &str) -> Option<i32> {
    body.split_whitespace()
        .find_map(|word| {
            word.strip_prefix("rc=")
                .or_else(|| word.strip_prefix("rc:"))
        })
        .and_then(|value| {
            value
                .trim_end_matches(|ch: char| !ch.is_ascii_digit() && ch != '-')
                .parse()
                .ok()
        })
}

impl Store {
    /// Resolve a lane without mailbox input. Completion remains `None` and is
    /// represented by `RuntimeDiagnostic::MissingCompletion`.
    pub fn resolve_lane_runtime(
        &self,
        lane: &str,
        routes: &BTreeMap<String, Route>,
    ) -> Result<LaneRuntime> {
        resolve(self, lane, routes, &[])
    }

    /// Resolve a lane and include folded mailbox rows for completion evidence.
    pub fn resolve_lane_runtime_with_messages(
        &self,
        lane: &str,
        routes: &BTreeMap<String, Route>,
        messages: &[Message],
    ) -> Result<LaneRuntime> {
        resolve(self, lane, routes, messages)
    }

    fn runtime_lane_exists(&self, lane: &str) -> Result<bool> {
        Ok(self.connection().query_row(
            "SELECT EXISTS(SELECT 1 FROM dict_session WHERE value = ?1)",
            rusqlite::params![lane],
            |row| row.get(0),
        )?)
    }

    fn runtime_trace_names(&self, lane: &str) -> Result<Vec<String>> {
        let sql = "SELECT DISTINCT t.value FROM agent_trace_span s
                   JOIN dict_session d ON d.id = s.session_id
                   JOIN dict_trace t ON t.id = s.trace_id
                   WHERE d.value = ?1
                   UNION
                   SELECT DISTINCT t.value FROM agent_lane l
                   JOIN dict_session d ON d.id = l.lane_id
                   JOIN dict_trace t ON t.id = l.trace_id
                   WHERE d.value = ?1
                   ORDER BY 1";
        let mut statement = self.connection().prepare(sql)?;
        let rows = statement.query_map(rusqlite::params![lane], |row| row.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<String>>>()?)
    }

    fn runtime_trace_root(&self, trace: &str) -> Result<Option<String>> {
        Ok(self
            .connection()
            .query_row(
                "SELECT d.value FROM agent_trace a
                   JOIN dict_trace t ON t.id = a.trace_id
                   LEFT JOIN dict_session d ON d.id = a.root_session_id
                  WHERE t.value = ?1",
                rusqlite::params![trace],
                |row| row.get(0),
            )
            .optional()?)
    }

    fn runtime_attached_sessions(&self, trace: &str) -> Result<Vec<AttachedSession>> {
        let sql = "SELECT d.value, span.attached_ts,
                          MAX(COALESCE(turns.last_ts, 0), COALESCE(usage.last_ts, 0)),
                          EXISTS(SELECT 1 FROM agent_session a WHERE a.session_id = span.session_id)
                     FROM agent_trace_span span
                     JOIN dict_trace trace ON trace.id = span.trace_id
                     JOIN dict_session d ON d.id = span.session_id
                     LEFT JOIN (SELECT session_id, MAX(ts) AS last_ts FROM agent_turn GROUP BY session_id) turns
                       ON turns.session_id = span.session_id
                     LEFT JOIN (SELECT session_id, MAX(ts) AS last_ts FROM agent_usage GROUP BY session_id) usage
                       ON usage.session_id = span.session_id
                    WHERE trace.value = ?1
                    GROUP BY span.session_id, d.value, span.attached_ts
                    ORDER BY span.attached_ts, d.value";
        let mut statement = self.connection().prepare(sql)?;
        let rows = statement.query_map(rusqlite::params![trace], |row| {
            Ok(AttachedSession {
                session: row.get(0)?,
                activity_ts: row.get(2)?,
                generated: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn runtime_process(&self, session: &str) -> Result<Option<ProcessIdentity>> {
        let sql = "SELECT status.value, live.pid, pane.value
                     FROM agent_live live
                     LEFT JOIN dict_session d ON d.id = live.session_id
                     LEFT JOIN dict_status status ON status.id = live.status_id
                     LEFT JOIN dict_pane pane ON pane.id = live.tmux_pane_id
                    WHERE d.value = ?1";
        let row = self
            .connection()
            .query_row(sql, rusqlite::params![session], |row| {
                let status: Option<String> = row.get(0)?;
                Ok(ProcessIdentity {
                    session: session.to_owned(),
                    alive: status.as_deref().map(|value| value == "live"),
                    status,
                    pid: row.get(1)?,
                    tmux_pane: row.get(2)?,
                })
            })
            .optional()?;
        Ok(row)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::bus::{Message, Route};
    use crate::Store;

    use super::{parse_exit_code, RuntimeDiagnostic};

    fn route(session_id: Option<&str>) -> Route {
        Route {
            kind: "lane".into(),
            harness: Some("opencode".into()),
            tmux: Some("lane-a".into()),
            cwd: Some("/repo".into()),
            model: None,
            mode: None,
            session_id: session_id.map(str::to_owned),
            source_path: None,
            parent: None,
            goal: Some("goal".into()),
            registered_at: Some("2026-08-14T00:00:00Z".into()),
        }
    }

    fn fresh_store(name: &str) -> (std::path::PathBuf, Store) {
        let path = std::env::temp_dir().join(format!(
            "boop-runtime-{name}-{}-{}.db",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let _ = std::fs::remove_file(&path);
        (path.clone(), Store::open(path).unwrap())
    }

    fn add_session(store: &Store, session: &str, ts: i64) {
        let session_id = store.intern_public("dict_session", session).unwrap();
        let harness_id = store.intern_public("dict_harness", "opencode").unwrap();
        store
            .connection()
            .execute(
                "INSERT INTO agent_session(session_id, harness_id, nickname, started_ts)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![session_id, harness_id, session, ts],
            )
            .unwrap();
        let role_id = store.intern_public("dict_role", "assistant").unwrap();
        store
            .connection()
            .execute(
                "INSERT INTO agent_turn(session_id, turn, ts, role_id, said)
                 VALUES (?1, 1, ?2, ?3, 'answer')",
                rusqlite::params![session_id, ts, role_id],
            )
            .unwrap();
    }

    #[test]
    fn parses_completion_exit_codes() {
        assert_eq!(parse_exit_code("lane x done rc=0"), Some(0));
        assert_eq!(parse_exit_code("lane x done rc:17"), Some(17));
        assert_eq!(parse_exit_code("lane x done"), None);
    }

    #[test]
    fn diagnostics_are_typed_and_serializable() {
        let diagnostic = RuntimeDiagnostic::AmbiguousCurrentSession {
            trace: "trace-a".into(),
            sessions: vec!["ses-a".into(), "ses-b".into()],
            activity_ts: 42,
        };
        assert_eq!(
            serde_json::to_value(diagnostic).unwrap()["kind"],
            "ambiguous_current_session"
        );
    }

    #[test]
    fn resolves_placeholder_to_latest_attached_session_and_completion() {
        let (path, store) = fresh_store("latest");
        store
            .attach_trace("lane-a", "trace-lane-a", "lane-create", 10)
            .unwrap();
        store
            .attach_trace("generated-1", "trace-lane-a", "supervisor-conversation", 11)
            .unwrap();
        store
            .attach_trace("generated-2", "trace-lane-a", "supervisor-conversation", 12)
            .unwrap();
        add_session(&store, "generated-1", 20);
        add_session(&store, "generated-2", 30);
        store
            .record_status("generated-2", 40, "live", Some(91), Some("%9"))
            .unwrap();
        let mut routes = BTreeMap::new();
        routes.insert("lane-a".into(), route(Some("generated-2")));
        let messages = vec![Message {
            id: "result-1".into(),
            from: "lane-a".into(),
            to: "parent".into(),
            from_timestamp: "2026-08-14T00:01:00Z".into(),
            to_timestamp: None,
            kind: "result".into(),
            reply_to: None,
            body: "lane lane-a done rc=0".into(),
            r#ref: None,
        }];
        let runtime = store
            .resolve_lane_runtime_with_messages("lane-a", &routes, &messages)
            .unwrap();
        assert_eq!(runtime.trace.as_deref(), Some("trace-lane-a"));
        assert_eq!(runtime.root_session.as_deref(), Some("lane-a"));
        assert_eq!(runtime.placeholder_session.as_deref(), Some("lane-a"));
        assert_eq!(
            runtime.generated_sessions,
            vec!["generated-1", "generated-2"]
        );
        assert_eq!(runtime.current_session.as_deref(), Some("generated-2"));
        assert_eq!(
            runtime.process.as_ref().and_then(|process| process.pid),
            Some(91)
        );
        assert_eq!(
            runtime.completion.as_ref().and_then(|row| row.exit_code),
            Some(0)
        );
        assert!(runtime.diagnostics.is_empty());
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn equal_activity_is_an_ambiguous_current_session() {
        let (path, store) = fresh_store("ambiguous");
        store
            .attach_trace("lane-a", "trace-lane-a", "lane-create", 10)
            .unwrap();
        store
            .attach_trace("generated-1", "trace-lane-a", "supervisor-conversation", 11)
            .unwrap();
        store
            .attach_trace("generated-2", "trace-lane-a", "supervisor-conversation", 12)
            .unwrap();
        add_session(&store, "generated-1", 20);
        add_session(&store, "generated-2", 20);
        let runtime = store
            .resolve_lane_runtime("lane-a", &BTreeMap::new())
            .unwrap();
        assert!(runtime.current_session.is_none());
        assert!(runtime.diagnostics.iter().any(|diagnostic| matches!(
            diagnostic,
            RuntimeDiagnostic::AmbiguousCurrentSession { sessions, .. }
                if sessions == &["generated-1".to_owned(), "generated-2".to_owned()]
        )));
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn missing_lane_and_trace_are_reported_as_typed_diagnostics() {
        let (path, store) = fresh_store("missing");
        let missing = store
            .resolve_lane_runtime("missing", &BTreeMap::new())
            .unwrap();
        assert!(missing
            .diagnostics
            .contains(&RuntimeDiagnostic::MissingLane));
        store.intern_public("dict_session", "lane-a").unwrap();
        let no_trace = store
            .resolve_lane_runtime("lane-a", &BTreeMap::new())
            .unwrap();
        assert!(no_trace
            .diagnostics
            .contains(&RuntimeDiagnostic::MissingTrace {
                lane: "lane-a".into()
            }));
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}
