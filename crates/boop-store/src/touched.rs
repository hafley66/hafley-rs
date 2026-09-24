//! What a set of sessions did on disk: the paths they touched and the
//! directories they ran in. A ⌘-clicked token an agent printed names one of these.

use anyhow::Result;
use rusqlite::types::Value;

use crate::Store;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionTouched {
    /// Distinct touched paths, newest touch first.
    pub paths: Vec<String>,
    /// Distinct turn cwds, newest turn first, then each session's launch cwd.
    pub cwds: Vec<String>,
}

/// The `ids` CTE: each session's dict id plus its subagents (`<parent>/<child>`).
/// A range on the UNIQUE index, never `LIKE`, which scans agent_turn (11 s on a 2 GB store).
fn session_ids(sessions: &[String]) -> (String, Vec<Value>) {
    let filter = (1..=sessions.len())
        .map(|n| format!("value = ?{n} OR (value >= ?{n} || '/' AND value < ?{n} || '0')"))
        .collect::<Vec<_>>()
        .join(" OR ");
    (
        format!("WITH ids AS (SELECT id FROM dict_session WHERE {filter})"),
        sessions.iter().cloned().map(Value::Text).collect(),
    )
}

fn push_distinct(out: &mut Vec<String>, value: String) {
    if !value.is_empty() && !out.contains(&value) {
        out.push(value);
    }
}

impl Store {
    pub fn session_touched(&self, sessions: &[String], limit: usize) -> Result<SessionTouched> {
        let mut touched = SessionTouched::default();
        if sessions.is_empty() {
            return Ok(touched);
        }
        let (ids, params) = session_ids(sessions);
        let connection = self.connection();
        let mut statement = connection.prepare(&format!(
            "{ids} SELECT p.value FROM agent_touch t
               JOIN dict_path p ON p.id = t.path_id
              WHERE t.session_id IN ids
              ORDER BY t.ts DESC, t.turn DESC LIMIT {limit}"
        ))?;
        for path in statement.query_map(rusqlite::params_from_iter(params.iter()), |row| row.get::<_, String>(0))? {
            push_distinct(&mut touched.paths, path?);
        }
        let mut statement = connection.prepare(&format!(
            "{ids} SELECT value FROM (
               SELECT c.value AS value, 0 AS tier, MAX(t.ts) AS at FROM agent_turn t
                 JOIN agent_session a ON a.session_id = t.session_id
                 JOIN dict_cwd c ON c.id = COALESCE(t.cwd_id, a.cwd_id)
                WHERE t.session_id IN ids
                GROUP BY c.value
               UNION ALL
               SELECT c.value, 1, a.started_ts FROM agent_session a
                 JOIN dict_cwd c ON c.id = a.cwd_id
                WHERE a.session_id IN ids)
             ORDER BY tier, at DESC"
        ))?;
        for cwd in statement.query_map(rusqlite::params_from_iter(params.iter()), |row| row.get::<_, String>(0))? {
            push_distinct(&mut touched.cwds, cwd?.trim_end_matches('/').to_owned());
        }
        Ok(touched)
    }

    /// Sessions launched in `cwd`, most recently active first: the fallback
    /// when no live registry names the session standing in a pane.
    pub fn sessions_in_cwd(&self, cwd: &str, limit: usize) -> Result<Vec<String>> {
        let mut statement = self.connection().prepare(&format!(
            "SELECT s.value FROM agent_session a
               JOIN dict_session s ON s.id = a.session_id
               JOIN dict_cwd c ON c.id = a.cwd_id
               LEFT JOIN agent_turn t ON t.session_id = a.session_id
              WHERE c.value = ?1 AND s.value NOT LIKE '%/%'
              GROUP BY a.session_id
              ORDER BY MAX(COALESCE(t.ts, a.started_ts, 0)) DESC LIMIT {limit}"
        ))?;
        let rows = statement.query_map([cwd.trim_end_matches('/')], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touched_paths_and_cwds_come_newest_first_with_subagents() {
        let dir = tempfile_dir();
        let store = Store::open(dir.join("boop.db")).unwrap();
        store
            .connection()
            .execute_batch(
                "INSERT INTO dict_session(id, value) VALUES (1, 'sess'), (2, 'sess/sub'), (3, 'other');
                 INSERT INTO dict_cwd(id, value) VALUES (1, '/repo'), (2, '/repo-wt/'), (3, '/elsewhere');
                 INSERT INTO dict_path(id, value) VALUES (1, '/repo/a.rs'), (2, '/repo-wt/b.rs'), (3, '/elsewhere/c.rs');
                 INSERT INTO dict_verb(id, value) VALUES (1, 'read');
                 INSERT INTO agent_session(session_id, harness_id, cwd_id, started_ts) VALUES (1, 1, 1, 10), (2, 1, 1, 11), (3, 1, 3, 12);
                 INSERT INTO agent_turn(session_id, turn, ts, role_id, cwd_id) VALUES (1, 1, 100, 1, NULL), (1, 2, 200, 1, 2), (2, 1, 150, 1, NULL), (3, 1, 300, 1, NULL);
                 INSERT INTO agent_touch(session_id, turn, ts, path_id, verb_id) VALUES (1, 1, 100, 1, 1), (2, 1, 150, 2, 1), (1, 2, 200, 1, 1), (3, 1, 300, 3, 1);",
            )
            .unwrap();
        let touched = store.session_touched(&["sess".to_owned()], 2000).unwrap();
        assert_eq!(
            touched,
            SessionTouched {
                paths: vec!["/repo/a.rs".into(), "/repo-wt/b.rs".into()],
                cwds: vec!["/repo-wt".into(), "/repo".into()],
            }
        );
        assert_eq!(store.session_touched(&[], 2000).unwrap(), SessionTouched::default());
        assert_eq!(store.sessions_in_cwd("/repo/", 3).unwrap(), vec!["sess".to_owned()]);
        let _ = std::fs::remove_dir_all(dir);
    }

    fn tempfile_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("boop-store-touched-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
