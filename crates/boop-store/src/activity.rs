//! Set-wise transcript and usage activity counts.
//!
//! The projection has three scopes. A session is one normalized transcript
//! identity, a trace is its attached-session set, and a lane is the trace set
//! recorded at lane spawn. Each scope reads the turn and usage tables once and
//! joins their per-session aggregates to a distinct membership relation.

use anyhow::Result;
use rusqlite::params_from_iter;
use serde::Serialize;

use crate::ident::Store;

/// The identity relation to aggregate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityScope {
    Session,
    Trace,
    Lane,
}

/// Whether normalized tool-result rows are available to count.
///
/// `agent_turn` retains user, assistant, tool-call, and other turn roles.
/// Current harness projectors omit tool-result blocks, so no numeric
/// tool-result count can be derived from this store version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolResultAvailability {
    Unavailable,
}

/// One typed activity projection row. `identity` is a session id, trace id,
/// or lane name according to `scope`; it is never a SQLite dictionary id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ActivityCount {
    pub scope: ActivityScope,
    pub identity: String,
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

const SESSION_MEMBERS: &str = r#"
    SELECT known.session_id, session.value AS identity
      FROM (
        SELECT session_id FROM agent_session
        UNION
        SELECT session_id FROM agent_turn
        UNION
        SELECT session_id FROM agent_usage
        UNION
        SELECT session_id FROM agent_trace_span
      ) AS known
      JOIN dict_session AS session ON session.id = known.session_id
"#;

const TRACE_MEMBERS: &str = r#"
    SELECT span.session_id, trace.value AS identity
      FROM (
        SELECT trace_id FROM agent_trace
        UNION
        SELECT trace_id FROM agent_trace_span
        UNION
        SELECT trace_id FROM agent_lane WHERE trace_id IS NOT NULL
      ) AS known
      JOIN dict_trace AS trace ON trace.id = known.trace_id
      LEFT JOIN agent_trace_span AS span ON span.trace_id = known.trace_id
"#;

const LANE_MEMBERS: &str = r#"
    SELECT span.session_id, lane.value AS identity
      FROM (
        SELECT DISTINCT lane_id, trace_id FROM agent_lane
      ) AS known
      JOIN dict_session AS lane ON lane.id = known.lane_id
      LEFT JOIN agent_trace_span AS span ON span.trace_id = known.trace_id
"#;

fn activity_sql(scope: ActivityScope) -> String {
    let members = match scope {
        ActivityScope::Session => SESSION_MEMBERS,
        ActivityScope::Trace => TRACE_MEMBERS,
        ActivityScope::Lane => LANE_MEMBERS,
    };
    format!(
        r#"
WITH
turn_counts AS (
    SELECT turn.session_id,
           COUNT(*) FILTER (WHERE role.value = 'user') AS user_count,
           COUNT(*) FILTER (WHERE role.value = 'assistant') AS assistant_count,
           COUNT(*) FILTER (WHERE role.value = 'tool') AS tool_count,
           COUNT(*) AS total_count,
           MIN(turn.ts) AS first_ts,
           MAX(turn.ts) AS last_ts
      FROM agent_turn AS turn
      JOIN dict_role AS role ON role.id = turn.role_id
     GROUP BY turn.session_id
),
usage_counts AS (
    SELECT session_id,
           COUNT(*) AS call_count,
           SUM(input_tokens) AS input_tokens,
           SUM(output_tokens) AS output_tokens,
           SUM(cache_create_5m_tokens) AS cache_create_5m_tokens,
           SUM(cache_create_1h_tokens) AS cache_create_1h_tokens,
           SUM(cache_read_tokens) AS cache_read_tokens,
           MIN(ts) AS first_ts,
           MAX(ts) AS last_ts
      FROM agent_usage
     GROUP BY session_id
),
members AS (
    SELECT DISTINCT session_id, identity FROM ({members})
)
SELECT members.identity,
       COALESCE(SUM(turn_counts.user_count), 0) AS user_count,
       COALESCE(SUM(turn_counts.assistant_count), 0) AS assistant_count,
       COALESCE(SUM(turn_counts.tool_count), 0) AS tool_count,
       COALESCE(SUM(turn_counts.total_count), 0) AS total_count,
       COALESCE(SUM(usage_counts.call_count), 0) AS call_count,
       COALESCE(SUM(usage_counts.input_tokens), 0) AS input_tokens,
       COALESCE(SUM(usage_counts.output_tokens), 0) AS output_tokens,
       COALESCE(SUM(usage_counts.cache_create_5m_tokens), 0) AS cache_create_5m_tokens,
       COALESCE(SUM(usage_counts.cache_create_1h_tokens), 0) AS cache_create_1h_tokens,
       COALESCE(SUM(usage_counts.cache_read_tokens), 0) AS cache_read_tokens,
       MIN(CASE
             WHEN turn_counts.first_ts IS NULL THEN usage_counts.first_ts
             WHEN usage_counts.first_ts IS NULL THEN turn_counts.first_ts
             WHEN turn_counts.first_ts < usage_counts.first_ts THEN turn_counts.first_ts
             ELSE usage_counts.first_ts
           END) AS first_activity_ts,
       MAX(CASE
             WHEN turn_counts.last_ts IS NULL THEN usage_counts.last_ts
             WHEN usage_counts.last_ts IS NULL THEN turn_counts.last_ts
             WHEN turn_counts.last_ts > usage_counts.last_ts THEN turn_counts.last_ts
             ELSE usage_counts.last_ts
           END) AS last_activity_ts
  FROM members
  LEFT JOIN turn_counts ON turn_counts.session_id = members.session_id
  LEFT JOIN usage_counts ON usage_counts.session_id = members.session_id
 GROUP BY members.identity
 ORDER BY members.identity
"#
    )
}

fn scoped_trace_activity_sql(trace_count: usize) -> String {
    let traces = (1..=trace_count)
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"
WITH
selected_traces(trace_id, identity) AS MATERIALIZED (
    SELECT trace.id, trace.value
      FROM dict_trace AS trace
     WHERE trace.value IN ({traces})
),
members(session_id, identity) AS MATERIALIZED (
    SELECT span.session_id, trace.identity
      FROM selected_traces AS trace
      LEFT JOIN agent_trace_span AS span ON span.trace_id = trace.trace_id
),
turn_counts AS (
    SELECT turn.session_id,
           COUNT(*) FILTER (WHERE role.value = 'user') AS user_count,
           COUNT(*) FILTER (WHERE role.value = 'assistant') AS assistant_count,
           COUNT(*) FILTER (WHERE role.value = 'tool') AS tool_count,
           COUNT(*) AS total_count,
           MIN(turn.ts) AS first_ts,
           MAX(turn.ts) AS last_ts
      FROM agent_turn AS turn
      JOIN dict_role AS role ON role.id = turn.role_id
     WHERE turn.session_id IN (
               SELECT session_id FROM members WHERE session_id IS NOT NULL
           )
     GROUP BY turn.session_id
),
usage_counts AS (
    SELECT usage.session_id,
           COUNT(*) AS call_count,
           SUM(usage.input_tokens) AS input_tokens,
           SUM(usage.output_tokens) AS output_tokens,
           SUM(usage.cache_create_5m_tokens) AS cache_create_5m_tokens,
           SUM(usage.cache_create_1h_tokens) AS cache_create_1h_tokens,
           SUM(usage.cache_read_tokens) AS cache_read_tokens,
           MIN(usage.ts) AS first_ts,
           MAX(usage.ts) AS last_ts
      FROM agent_usage AS usage
     WHERE usage.session_id IN (
               SELECT session_id FROM members WHERE session_id IS NOT NULL
           )
     GROUP BY usage.session_id
)
SELECT members.identity,
       COALESCE(SUM(turn_counts.user_count), 0) AS user_count,
       COALESCE(SUM(turn_counts.assistant_count), 0) AS assistant_count,
       COALESCE(SUM(turn_counts.tool_count), 0) AS tool_count,
       COALESCE(SUM(turn_counts.total_count), 0) AS total_count,
       COALESCE(SUM(usage_counts.call_count), 0) AS call_count,
       COALESCE(SUM(usage_counts.input_tokens), 0) AS input_tokens,
       COALESCE(SUM(usage_counts.output_tokens), 0) AS output_tokens,
       COALESCE(SUM(usage_counts.cache_create_5m_tokens), 0) AS cache_create_5m_tokens,
       COALESCE(SUM(usage_counts.cache_create_1h_tokens), 0) AS cache_create_1h_tokens,
       COALESCE(SUM(usage_counts.cache_read_tokens), 0) AS cache_read_tokens,
       MIN(CASE
             WHEN turn_counts.first_ts IS NULL THEN usage_counts.first_ts
             WHEN usage_counts.first_ts IS NULL THEN turn_counts.first_ts
             WHEN turn_counts.first_ts < usage_counts.first_ts THEN turn_counts.first_ts
             ELSE usage_counts.first_ts
           END) AS first_activity_ts,
       MAX(CASE
             WHEN turn_counts.last_ts IS NULL THEN usage_counts.last_ts
             WHEN usage_counts.last_ts IS NULL THEN turn_counts.last_ts
             WHEN turn_counts.last_ts > usage_counts.last_ts THEN turn_counts.last_ts
             ELSE usage_counts.last_ts
           END) AS last_activity_ts
  FROM members
  LEFT JOIN turn_counts ON turn_counts.session_id = members.session_id
  LEFT JOIN usage_counts ON usage_counts.session_id = members.session_id
 GROUP BY members.identity
 ORDER BY members.identity
"#
    )
}

fn count(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(index)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
}

fn activity_count(
    scope: ActivityScope,
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ActivityCount> {
    Ok(ActivityCount {
        scope,
        identity: row.get(0)?,
        user: count(row, 1)?,
        assistant: count(row, 2)?,
        tool_call: count(row, 3)?,
        total: count(row, 4)?,
        calls: count(row, 5)?,
        input_tokens: row.get(6)?,
        output_tokens: row.get(7)?,
        cache_create_5m_tokens: row.get(8)?,
        cache_create_1h_tokens: row.get(9)?,
        cache_read_tokens: row.get(10)?,
        first_activity_ts: row.get(11)?,
        last_activity_ts: row.get(12)?,
        tool_result_availability: ToolResultAvailability::Unavailable,
    })
}

impl Store {
    /// Aggregate normalized turn and usage facts by the requested identity
    /// scope. The SQL has a fixed number of aggregate scans, independent of
    /// the number of returned sessions, traces, or lanes.
    pub fn activity_counts(&self, scope: ActivityScope) -> Result<Vec<ActivityCount>> {
        let mut statement = self.connection().prepare(&activity_sql(scope))?;
        let rows = statement.query_map([], |row| activity_count(scope, row))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Aggregate only the selected trace identities. Agent summary uses this
    /// internal projection after its runtime pass establishes the trace
    /// boundary, so unrelated transcript tables are never aggregated.
    pub fn activity_counts_for_traces(&self, traces: &[String]) -> Result<Vec<ActivityCount>> {
        if traces.is_empty() {
            return Ok(Vec::new());
        }
        let sql = scoped_trace_activity_sql(traces.len());
        let mut statement = self.connection().prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(traces), |row| {
            activity_count(ActivityScope::Trace, row)
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::ident::{LaneSpawn, UsageRow};
    use crate::{ActivityScope, Store, ToolResultAvailability};

    use super::{activity_sql, scoped_trace_activity_sql};

    fn fresh_store(name: &str) -> (PathBuf, Store) {
        let path = std::env::temp_dir().join(format!(
            "boop-activity-{name}-{}-{}.db",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let _ = std::fs::remove_file(&path);
        (path.clone(), Store::open(path).unwrap())
    }

    fn session(store: &Store, id: &str, harness: &str, started_ts: i64) {
        let session_id = store.intern_public("dict_session", id).unwrap();
        let harness_id = store.intern_public("dict_harness", harness).unwrap();
        store
            .connection()
            .execute(
                "INSERT INTO agent_session(session_id, harness_id, nickname, started_ts)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![session_id, harness_id, id, started_ts],
            )
            .unwrap();
    }

    fn turn(store: &Store, session: &str, ordinal: i64, ts: i64, role: &str) {
        let session_id = store.intern_public("dict_session", session).unwrap();
        let role_id = store.intern_public("dict_role", role).unwrap();
        store
            .connection()
            .execute(
                "INSERT INTO agent_turn(session_id, turn, ts, role_id, said)
                 VALUES (?1, ?2, ?3, ?4, '')",
                rusqlite::params![session_id, ordinal, ts, role_id],
            )
            .unwrap();
    }

    fn usage(store: &Store, session: &str, turn: u64, ts: u64, request: &str, output: i64) {
        let row = UsageRow {
            ts,
            message_id: request,
            request_id: request,
            model: "model",
            service_tier: None,
            input_tokens: 10,
            output_tokens: output,
            cache_create_5m_tokens: 2,
            cache_create_1h_tokens: 3,
            cache_read_tokens: 4,
            is_sidechain: false,
            cost_usd_recorded: None,
        };
        store.write_usage(session, turn, &row).unwrap();
    }

    fn lane(store: &Store, name: &str, trace: Option<&str>, ts: u64) {
        store
            .record_lane_spawn(&LaneSpawn {
                lane: name.into(),
                trace: trace.map(str::to_owned),
                ts,
                ..LaneSpawn::default()
            })
            .unwrap();
    }

    #[test]
    fn projects_claude_codex_opencode_kimi_resume_replacement_and_shell_lanes() {
        let (path, store) = fresh_store("all-harnesses");
        for (id, harness, started) in [
            ("claude-root", "claude", 10),
            ("claude-resume", "claude", 20),
            ("codex-original", "codex", 30),
            ("codex-replacement", "codex", 40),
            ("opencode-generated", "opencode", 50),
            ("kimi-child", "kimi", 60),
        ] {
            session(&store, id, harness, started);
        }

        for (session_id, trace, attach_ts) in [
            ("claude-root", "trace-claude", 10),
            ("claude-resume", "trace-claude", 20),
            ("codex-original", "trace-codex", 30),
            ("codex-replacement", "trace-codex", 40),
            ("opencode-generated", "trace-opencode", 50),
            ("kimi-child", "trace-kimi", 60),
        ] {
            store
                .attach_trace(session_id, trace, "fixture", attach_ts)
                .unwrap();
        }
        lane(&store, "lane-claude", Some("trace-claude"), 10);
        lane(&store, "lane-codex", Some("trace-codex"), 30);
        lane(&store, "lane-opencode", Some("trace-opencode"), 50);
        lane(&store, "lane-kimi", Some("trace-kimi"), 60);
        lane(&store, "lane-shell", None, 70);

        turn(&store, "claude-root", 1, 11, "user");
        turn(&store, "claude-resume", 1, 21, "assistant");
        turn(&store, "codex-original", 1, 31, "user");
        turn(&store, "codex-replacement", 1, 41, "tool");
        turn(&store, "codex-replacement", 2, 42, "assistant");
        turn(&store, "opencode-generated", 1, 51, "assistant");
        turn(&store, "kimi-child", 1, 61, "tool");
        usage(&store, "claude-root", 1, 12, "claude-call", 20);
        usage(&store, "codex-replacement", 2, 43, "codex-call", 30);
        usage(&store, "opencode-generated", 1, 52, "opencode-call", 40);
        usage(&store, "kimi-child", 1, 62, "kimi-call", 50);

        let sessions = store.activity_counts(ActivityScope::Session).unwrap();
        let replacement = sessions
            .iter()
            .find(|row| row.identity == "codex-replacement")
            .unwrap();
        assert_eq!(replacement.user, 0);
        assert_eq!(replacement.assistant, 1);
        assert_eq!(replacement.tool_call, 1);
        assert_eq!(replacement.total, 2);
        assert_eq!(replacement.calls, 1);
        assert_eq!(replacement.input_tokens, 10);
        assert_eq!(replacement.output_tokens, 30);
        assert_eq!(replacement.first_activity_ts, Some(41));
        assert_eq!(replacement.last_activity_ts, Some(43));
        assert_eq!(
            replacement.tool_result_availability,
            ToolResultAvailability::Unavailable
        );

        let traces = store.activity_counts(ActivityScope::Trace).unwrap();
        let codex_trace = traces
            .iter()
            .find(|row| row.identity == "trace-codex")
            .unwrap();
        assert_eq!(codex_trace.user, 1);
        assert_eq!(codex_trace.assistant, 1);
        assert_eq!(codex_trace.tool_call, 1);
        assert_eq!(codex_trace.total, 3);
        assert_eq!(codex_trace.calls, 1);
        assert_eq!(codex_trace.output_tokens, 30);
        assert_eq!(codex_trace.first_activity_ts, Some(31));
        assert_eq!(codex_trace.last_activity_ts, Some(43));

        let lanes = store.activity_counts(ActivityScope::Lane).unwrap();
        let codex_lane = lanes
            .iter()
            .find(|row| row.identity == "lane-codex")
            .unwrap();
        assert_eq!(codex_lane.scope, ActivityScope::Lane);
        assert_eq!(codex_lane.user, codex_trace.user);
        assert_eq!(codex_lane.assistant, codex_trace.assistant);
        assert_eq!(codex_lane.tool_call, codex_trace.tool_call);
        assert_eq!(codex_lane.total, codex_trace.total);
        assert_eq!(codex_lane.calls, codex_trace.calls);
        assert_eq!(codex_lane.output_tokens, codex_trace.output_tokens);
        let shell = lanes
            .iter()
            .find(|row| row.identity == "lane-shell")
            .unwrap();
        assert_eq!(shell.total, 0);
        assert_eq!(shell.calls, 0);
        assert_eq!(shell.first_activity_ts, None);
        assert_eq!(shell.last_activity_ts, None);

        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn aggregation_query_has_no_per_identity_subquery() {
        let (path, store) = fresh_store("query-plan");
        for scope in [
            ActivityScope::Session,
            ActivityScope::Trace,
            ActivityScope::Lane,
        ] {
            let sql = format!("EXPLAIN QUERY PLAN {}", activity_sql(scope));
            let mut statement = store.connection().prepare(&sql).unwrap();
            let plan = statement
                .query_map([], |row| row.get::<_, String>(3))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
                .join("\n")
                .to_ascii_uppercase();
            assert!(
                !plan.contains("CORRELATED SCALAR SUBQUERY"),
                "{scope:?} activity query has per-identity work:\n{plan}"
            );
        }
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn scoped_trace_counts_seek_selected_sessions_only() {
        let (path, store) = fresh_store("scoped-trace");
        for (session_id, harness, turn_ts, usage_ts) in [
            ("selected-claude", "claude", 10, 11),
            ("selected-codex", "codex", 12, 13),
        ] {
            session(&store, session_id, harness, turn_ts);
            store
                .attach_trace(session_id, "trace-selected", "fixture", turn_ts as u64)
                .unwrap();
            turn(&store, session_id, 1, turn_ts, "assistant");
            usage(
                &store,
                session_id,
                1,
                usage_ts as u64,
                &format!("request-{session_id}"),
                20,
            );
        }
        for number in 0..7 {
            let session_id = format!("unrelated-{number}");
            store
                .attach_trace(
                    &session_id,
                    &format!("trace-unrelated-{number}"),
                    "fixture",
                    20,
                )
                .unwrap();
            turn(&store, &session_id, 1, 20, "assistant");
            usage(&store, &session_id, 1, 21, &format!("request-{number}"), 30);
        }
        let selected = vec!["trace-selected".to_owned()];
        let scoped = store.activity_counts_for_traces(&selected).unwrap();
        let global = store
            .activity_counts(ActivityScope::Trace)
            .unwrap()
            .into_iter()
            .filter(|row| row.identity == "trace-selected")
            .collect::<Vec<_>>();
        assert_eq!(scoped, global);
        assert_eq!(scoped[0].assistant, 2);
        assert_eq!(scoped[0].calls, 2);

        let sql = format!(
            "EXPLAIN QUERY PLAN {}",
            scoped_trace_activity_sql(selected.len())
        );
        let mut statement = store.connection().prepare(&sql).unwrap();
        let plan = statement
            .query_map(rusqlite::params!["trace-selected"], |row| {
                row.get::<_, String>(3)
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
            .join("\n")
            .to_ascii_uppercase();
        assert!(plan.contains("SEARCH TURN USING PRIMARY KEY"), "{plan}");
        assert!(
            plan.contains("SEARCH SPAN USING COVERING INDEX IDX_SPAN_TRACE"),
            "{plan}"
        );
        assert!(plan.contains("SEARCH USAGE USING PRIMARY KEY"), "{plan}");
        assert!(!plan.contains("SCAN TURN"), "{plan}");
        assert!(!plan.contains("SCAN USAGE"), "{plan}");
        drop(statement);
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}
