//! Schema 40 closed vocabularies and the atomic in-place migration.
use crate::Store;
use anyhow::Context;
use anyhow::{ensure, Result};
pub(crate) const SCHEMA: &str = include_str!("../sql/40_schema.sql");

pub(crate) const REFERENCES: &[(&str, &str, &str)] = &[
    ("agent_trace_span", "attach_id", "dict_attach"),
    ("agent_session_attr", "key_id", "dict_attr_key"),
    ("agent_edge", "edge_kind_id", "dict_edekind"),
    ("agent_fetch", "kind_id", "dict_netkind"),
    ("agent_turn", "role_id", "dict_role"),
    ("agent_touch", "verb_id", "dict_verb"),
    ("agent_touch", "raw_verb_id", "dict_verb"),
    (
        "agent_session_relation",
        "kind_id",
        "dict_session_relation_kind",
    ),
    (
        "agent_session_relation",
        "source_id",
        "dict_observation_source",
    ),
    ("agent_session_observation", "harness_id", "dict_harness"),
    (
        "agent_session_observation",
        "source_id",
        "dict_observation_source",
    ),
    ("model_price", "source_id", "dict_price_source"),
    ("agent_trace_event", "kind_id", "dict_trace_kind"),
    (
        "agent_trace_event",
        "delivery_state_id",
        "dict_trace_delivery",
    ),
    (
        "agent_trace_event",
        "classification_id",
        "dict_trace_classification",
    ),
    ("agent_live", "status_id", "dict_status"),
    ("agent_live_span", "status_id", "dict_status"),
    ("agent_session", "harness_id", "dict_harness"),
    ("agent_lane", "harness_id", "dict_harness"),
    ("agent_delivery", "harness_id", "dict_harness"),
    ("agent_delivery_transition", "harness_id", "dict_harness"),
    ("sync_root_stamp", "harness_id", "dict_harness"),
];

pub fn value(domain: &str, value: &str) -> Result<&'static str> {
    let allowed: &'static [&'static str] = match domain {
        "dict_attr_key" => &[
            "effort",
            "reset_ts",
            "process_pid",
            "process_start_secs",
            "process_previous_session",
            "mood",
        ],
        "dict_harness" => &["claude", "codex", "kimi", "opencode", "gemini", "omp"],
        "dict_observation_source" => &[
            "legacy-agent-trace-span",
            "trace-event",
            "transcript-sync",
            "trace-attach",
            "live-status",
            "transcript-session-metadata",
        ],
        "dict_role" => &["user", "assistant", "tool", "system", "developer", "meta"],
        "dict_session_relation_kind" => &[
            "continued-in",
            "same-process",
            "parent-child",
            "same-pane",
            "same-tui",
        ],
        "dict_status" => &["idle", "live", "detached", "dead", "closed"],
        "dict_price_source" => &["litellm", "openrouter"],
        "dict_attach" => &[
            "backfill-spawned-edge",
            "lane-create",
            "lane-run",
            "supervisor-conversation",
            "native-tui-session",
            "derived-session-relation",
        ],
        "dict_edekind" => &[
            "spawned",
            "result",
            "deliver-nextturn",
            "hail",
            "deliver-midturn",
            "completed",
            "completion-mailed",
            "completion-delivered",
            "cancel",
        ],
        "dict_netkind" => &["fetch", "search"],
        "dict_trace_classification" => &[
            "starting",
            "opened",
            "started",
            "failed",
            "completed",
            "retryable",
            "delivered",
            "queued",
            "retired",
            "quiet",
            "active",
            "same-process",
        ],
        "dict_trace_delivery" => &[
            "midturn",
            "nextturn",
            "started",
            "ok",
            "version",
            "help",
            "parse-error",
            "error",
        ],
        "dict_trace_kind" => &[
            "supervisor-start",
            "channel-open",
            "turn-start",
            "error",
            "supervisor-exit",
            "turn-finish",
            "delivery",
            "idle-shutdown",
            "cli-invocation",
            "harness-quiet",
            "harness-active",
            "session-boundary",
        ],
        "dict_verb" => &[
            "read", "Read", "edit", "Edit", "write", "Write", "grep", "Grep", "glob", "Glob",
        ],
        _ => anyhow::bail!("unknown enum domain {domain}"),
    };
    ensure!(allowed.contains(&value), "invalid {domain} value {value:?}");
    Ok(*allowed
        .iter()
        .find(|candidate| **candidate == value)
        .unwrap())
}

impl Store {
    /// Runs inside the schema owner's transaction, after schema 39. A CHECK
    /// failure or unresolved mandatory dictionary reference rolls everything back.
    pub(crate) fn migrate_closed_sets(&self) -> Result<()> {
        let mut counts = std::collections::BTreeMap::new();
        for &(table, column, dictionary) in REFERENCES {
            let unresolved: i64 = self.connection().query_row(
                &format!("SELECT count(*) FROM {table} t LEFT JOIN {dictionary} d ON d.id=t.{column} WHERE t.{column} IS NOT NULL AND d.id IS NULL"),
                [], |row| row.get(0),
            )?;
            ensure!(
                unresolved == 0,
                "schema 40: {table}.{column} has {unresolved} unresolved {dictionary} references"
            );
            if !counts.contains_key(table) {
                let count: i64 = self.connection().query_row(
                    &format!("SELECT count(*) FROM {table}"),
                    [],
                    |row| row.get(0),
                )?;
                counts.insert(table, count);
            }
        }
        for statement in include_str!("../sql/40_closed_sets.sql").split(";\n\n") {
            self.connection()
                .execute_batch(statement)
                .with_context(|| {
                    format!(
                        "schema 40: {}",
                        statement
                            .lines()
                            .find(|line| !line.is_empty())
                            .unwrap_or("empty")
                    )
                })?;
        }
        for (table, before) in counts {
            let after: i64 = self.connection().query_row(
                &format!("SELECT count(*) FROM {table}"),
                [],
                |row| row.get(0),
            )?;
            ensure!(
                before == after,
                "schema 40: {table} row count changed from {before} to {after}"
            );
        }
        Ok(())
    }
}
