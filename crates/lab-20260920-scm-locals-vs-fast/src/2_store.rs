use rusqlite::{params, Connection};

use crate::{LabError, NodeKind};

pub const RESOLVE_SQL: &str = r#"
WITH RECURSIVE walk(id, stack, depth, seen) AS (
    SELECT ?1, '', 0, printf(',%d,', ?1)
    UNION ALL
    SELECT edge.dst,
           CASE node.kind
             WHEN 'push' THEN walk.stack || replace(node.sym, '|', '') || '|'
             WHEN 'pop' THEN substr(walk.stack, 1,
                                    length(walk.stack) - length(node.sym) - 1)
             ELSE walk.stack
           END,
           walk.depth + 1,
           walk.seen || printf('%d,', edge.dst)
      FROM walk
      JOIN edge ON edge.src = walk.id
      JOIN node ON node.id = edge.dst
     WHERE walk.depth < 64
       AND instr(walk.seen, printf(',%d,', edge.dst)) = 0
       AND (node.kind <> 'pop'
            OR substr(walk.stack, length(walk.stack) - length(node.sym),
                      length(node.sym)) = node.sym)
)
SELECT node.id, node.sym, node.blob, node.span_start, node.span_end, walk.depth
  FROM walk
  JOIN node ON node.id = walk.id
 WHERE node.kind = 'def' AND walk.stack = ''
 ORDER BY walk.depth, node.blob, node.span_start
"#;

pub struct Store {
    pub db: Connection,
}

impl Store {
    pub fn memory() -> Result<Self, LabError> {
        let db = Connection::open_in_memory().map_err(sql)?;
        db.execute_batch(
            "CREATE TABLE node(
                id INTEGER PRIMARY KEY,
                kind TEXT NOT NULL CHECK(kind IN
                  ('root','scope','def','ref','push','pop','export','import')),
                sym TEXT NOT NULL,
                blob TEXT NOT NULL,
                span_start INTEGER NOT NULL,
                span_end INTEGER NOT NULL
             );
             CREATE TABLE edge(src INTEGER NOT NULL, dst INTEGER NOT NULL,
                PRIMARY KEY(src, dst));",
        )
        .map_err(sql)?;
        Ok(Self { db })
    }

    pub fn node(
        &self,
        kind: NodeKind,
        sym: &str,
        blob: &str,
        start: u32,
        end: u32,
    ) -> Result<i64, LabError> {
        self.db
            .execute(
                "INSERT INTO node(kind,sym,blob,span_start,span_end) VALUES(?1,?2,?3,?4,?5)",
                params![kind.as_str(), sym, blob, start, end],
            )
            .map_err(sql)?;
        Ok(self.db.last_insert_rowid())
    }

    pub fn edge(&self, src: i64, dst: i64) -> Result<(), LabError> {
        self.db
            .execute(
                "INSERT OR IGNORE INTO edge(src,dst) VALUES(?1,?2)",
                [src, dst],
            )
            .map_err(sql)?;
        Ok(())
    }

    pub fn resolve(&self, reference: i64) -> Result<Vec<(String, String, u32, u32)>, LabError> {
        let mut statement = self.db.prepare(RESOLVE_SQL).map_err(sql)?;
        let rows = statement
            .query_map([reference], |row| {
                Ok((row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
            })
            .map_err(sql)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(sql)
    }
}

fn sql(error: rusqlite::Error) -> LabError {
    LabError::Sql(error.to_string())
}
