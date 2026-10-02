use super::*;
use crate::_0_trace_identity::{derive_traces, TraceJoinLimits};

impl Store {
    pub(super) fn rebuild_trace_projection(&self) -> Result<()> {
        let limits = TraceJoinLimits::default();
        let relation_count: usize = self.connection.query_row(
            "SELECT COUNT(*) FROM agent_session_relation",
            [],
            |row| row.get::<_, i64>(0).map(|count| count as usize),
        )?;
        if relation_count > limits.max_relations {
            return Err(anyhow::Error::new(
                crate::_0_trace_identity::TraceJoinDiagnostic::RelationBudgetExceeded {
                    limit: limits.max_relations,
                    observed: relation_count,
                },
            ));
        }
        if relation_count == 0 {
            return Ok(());
        }
        // Derivation reads only session identity and earliest timestamp. Repeated
        // transcript syncs add evidence rows, but never add graph vertices.
        let observations = {
            let mut statement = self.connection.prepare(
                "SELECT session.value, MIN(observation.observed_ts)
                   FROM agent_session_observation observation
                   JOIN dict_session session ON session.id = observation.session_id
                  GROUP BY observation.session_id
                  ORDER BY observation.session_id
                  LIMIT ?1",
            )?;
            let rows =
                statement.query_map(params![(limits.max_observations + 1) as i64], |row| {
                    Ok(SessionObservation {
                        observation_key: String::new(),
                        session_id: row.get(0)?,
                        observed_at_ms: row.get::<_, i64>(1)? as u64,
                        harness: None,
                        cwd: None,
                        pid: None,
                        parent_pid: None,
                        pane_id: None,
                        tui_session_id: None,
                        source: String::new(),
                    })
                })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let relations = {
            let mut statement = self.connection.prepare(
                "SELECT relation.relation_key, source.value, target.value, relation.kind,
                        relation.source, relation.observed_ts, relation.matched_identity_key
                   FROM agent_session_relation relation
                   JOIN dict_session source ON source.id = relation.from_session
                   JOIN dict_session target ON target.id = relation.to_session

                  ORDER BY relation.observed_ts, relation.relation_id",
            )?;
            let rows = statement.query_map([], |row| {
                let kind: String = row.get(3)?;
                Ok(SessionRelation {
                    relation_key: row.get(0)?,
                    from_session: row.get(1)?,
                    to_session: row.get(2)?,
                    kind: SessionRelationKind::from_str(&kind).ok_or_else(|| {
                        rusqlite::Error::InvalidColumnType(
                            3,
                            "kind".into(),
                            rusqlite::types::Type::Text,
                        )
                    })?,
                    source: row.get(4)?,
                    observed_at_ms: row.get::<_, i64>(5)? as u64,
                    matched_identity_key: row.get(6)?,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let traces =
            derive_traces(&observations, &relations, limits).map_err(anyhow::Error::new)?;
        let derived_attach = crate::closed_sets::value("dict_attach", "derived-session-relation")?;
        for trace in traces.iter().filter(|trace| !trace.evidence.is_empty()) {
            let mut existing = BTreeMap::new();
            for session in &trace.sessions {
                let attached: Option<(i64, i64)> = self
                    .connection
                    .query_row(
                        "SELECT span.trace_id, trace.started_ts
                           FROM agent_trace_span span
                           JOIN dict_session member ON member.id = span.session_id
                           JOIN agent_trace trace ON trace.trace_id = span.trace_id
                          WHERE member.value = ?1",
                        params![session],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?;
                if let Some((trace_id, started_ts)) = attached {
                    existing.entry(trace_id).or_insert(started_ts);
                }
            }
            if existing.len() > 1 {
                tracing::warn!(
                    diagnostic = "conflicting-existing-trace-attachments",
                    session_count = trace.sessions.len(),
                    trace_count = existing.len(),
                    "trace projection skipped a relation component with conflicting existing spans"
                );
                continue;
            }
            let trace_id = if let Some((&trace_id, _)) = existing.iter().next() {
                trace_id
            } else {
                self.intern("dict_trace", &format!("trace-{}", trace.root_session))?
            };
            let root_id = self.session_id(&trace.root_session)?;
            let started_ts = observations
                .iter()
                .filter(|observation| trace.sessions.contains(&observation.session_id))
                .map(|observation| observation.observed_at_ms as i64)
                .min()
                .unwrap_or(0);
            self.connection.execute(
                "INSERT OR IGNORE INTO agent_trace (trace_id, root_session_id, started_ts)
                 VALUES (?1, ?2, ?3)",
                params![trace_id, root_id, started_ts],
            )?;
            for session in &trace.sessions {
                let session_id = self.session_id(session)?;
                let attached_ts = observations
                    .iter()
                    .filter(|observation| observation.session_id == *session)
                    .map(|observation| observation.observed_at_ms as i64)
                    .min()
                    .or_else(|| {
                        trace
                            .evidence
                            .iter()
                            .filter(|relation| {
                                relation.from_session == *session || relation.to_session == *session
                            })
                            .map(|relation| relation.observed_at_ms as i64)
                            .min()
                    })
                    .unwrap_or(started_ts);
                self.connection.execute(
                    "INSERT INTO agent_trace_span
                       (session_id, trace_id, attach, attached_ts)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(session_id) DO UPDATE SET
                       trace_id = excluded.trace_id,
                       attach = excluded.attach",
                    params![session_id, trace_id, derived_attach, attached_ts],
                )?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history_store() -> Store {
        let store = Store::open(":memory:".into()).unwrap();
        let session = store.session_id("before").unwrap();
        store
            .connection
            .execute(
                "WITH RECURSIVE sequence(value) AS (
                 VALUES(1) UNION ALL SELECT value + 1 FROM sequence WHERE value < 10001
             ) INSERT INTO agent_session_observation
                 (observation_key, session_id, observed_ts, source)
               SELECT 'sync:' || value, ?1, 10002 - value, 'transcript-sync' FROM sequence",
                params![session],
            )
            .unwrap();
        store
            .record_session_observation(&SessionObservation {
                observation_key: "after".into(),
                session_id: "after".into(),
                observed_at_ms: 20,
                harness: None,
                cwd: None,
                pid: None,
                parent_pid: None,
                pane_id: None,
                tui_session_id: None,
                source: "transcript-sync".into(),
            })
            .unwrap();
        store
    }

    fn continuation() -> SessionRelation {
        SessionRelation {
            relation_key: "continue".into(),
            from_session: "before".into(),
            to_session: "after".into(),
            kind: SessionRelationKind::ContinuedIn,
            source: "transcript-session-metadata".into(),
            observed_at_ms: 30,
            matched_identity_key: None,
        }
    }

    #[test]
    fn repeated_history_preserves_earliest_timestamps_and_trace_membership() {
        let store = history_store();
        store.record_session_relation(&continuation()).unwrap();
        store.rebuild_trace_projection().unwrap();
        let mut statement = store.connection.prepare(
            "SELECT session.value, trace.value, root.value, identity.started_ts, span.attached_ts
               FROM agent_trace_span span
               JOIN dict_session session ON session.id = span.session_id
               JOIN dict_trace trace ON trace.id = span.trace_id
               JOIN agent_trace identity ON identity.trace_id = span.trace_id
               JOIN dict_session root ON root.id = identity.root_session_id
              ORDER BY session.value",
        ).unwrap();
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![
                (
                    "after".into(),
                    "trace-before".into(),
                    "before".into(),
                    1,
                    20
                ),
                (
                    "before".into(),
                    "trace-before".into(),
                    "before".into(),
                    1,
                    1
                )
            ]
        );
        assert_eq!(
            store
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM agent_session_observation",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            10002
        );
    }

    #[test]
    fn history_without_relations_leaves_existing_spans_unchanged() {
        let store = history_store();
        store
            .attach_trace("before", "kept", "lane-create", 50)
            .unwrap();
        store.rebuild_trace_projection().unwrap();
        assert_eq!(store.trace_of("before").unwrap(), Some("kept".into()));
        assert_eq!(store.trace_of("after").unwrap(), None);
    }

    #[test]
    fn distinct_session_history_still_exceeds_the_bounded_join() {
        let store = history_store();
        store.connection.execute_batch(
            "WITH RECURSIVE sequence(value) AS (
                 VALUES(1) UNION ALL SELECT value + 1 FROM sequence WHERE value < 10001
             ) INSERT INTO dict_session(value) SELECT 'distinct:' || value FROM sequence;
             INSERT INTO agent_session_observation (observation_key, session_id, observed_ts, source)
               SELECT value, id, 1, 'transcript-sync' FROM dict_session WHERE value LIKE 'distinct:%';",
        ).unwrap();
        let error = store.record_session_relation(&continuation()).unwrap_err();
        assert_eq!(
            error.to_string(),
            "trace join observation budget exceeded: 10001 > 10000"
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM agent_session_relation", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
