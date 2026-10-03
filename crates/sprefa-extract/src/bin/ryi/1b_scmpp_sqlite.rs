//! The SQLite writer behind the scm++ `Rows` seam: DDL, batched multi-row inserts, post-load indexes.
use rusqlite::{params_from_iter, Connection};

use super::rows::{CaptureRow, Dict, NodeRow, Result, Rows};

/// Integer keys only; the one string index is each dictionary's own UNIQUE(text).
pub const DDL: &str = "\
CREATE TABLE scmpp_dict_path (id INTEGER PRIMARY KEY, text TEXT NOT NULL UNIQUE);
CREATE TABLE scmpp_dict_kind (id INTEGER PRIMARY KEY, text TEXT NOT NULL UNIQUE);
CREATE TABLE scmpp_dict_field (id INTEGER PRIMARY KEY, text TEXT NOT NULL UNIQUE);
CREATE TABLE scmpp_dict_capture (id INTEGER PRIMARY KEY, text TEXT NOT NULL UNIQUE);
CREATE TABLE scmpp_dict_text (id INTEGER PRIMARY KEY, text TEXT NOT NULL UNIQUE);
CREATE TABLE scmpp_node (file INTEGER NOT NULL, pre INTEGER NOT NULL, last INTEGER NOT NULL, \
parent INTEGER NOT NULL, depth INTEGER NOT NULL, sib INTEGER, idx INTEGER NOT NULL, kind INTEGER NOT NULL, \
field INTEGER NOT NULL, start INTEGER NOT NULL, \"end\" INTEGER NOT NULL, named INTEGER NOT NULL, \
PRIMARY KEY (file, pre)) WITHOUT ROWID;
CREATE TABLE scmpp_capture (file INTEGER NOT NULL, pattern INTEGER NOT NULL, \"match\" INTEGER NOT NULL, \
capture INTEGER NOT NULL, node INTEGER NOT NULL, start INTEGER NOT NULL, \"end\" INTEGER NOT NULL, text INTEGER, \
PRIMARY KEY (file, pattern, \"match\", capture, node)) WITHOUT ROWID;";

/// Built after the load, before the one query.
pub const INDEXES: &str = "\
CREATE INDEX IF NOT EXISTS scmpp_node_sibling ON scmpp_node(file, parent, sib);
CREATE INDEX IF NOT EXISTS scmpp_capture_node ON scmpp_capture(pattern, capture, file, node);";

const NODE_COLUMNS: usize = 12;
const CAPTURE_COLUMNS: usize = 8;
const BATCH: usize = 256;

fn insert(table: &str, columns: usize, rows: usize) -> String {
    let row = format!("({})", vec!["?"; columns].join(","));
    format!("INSERT INTO {table} VALUES {}", vec![row; rows].join(","))
}

fn dict_table(dict: Dict) -> &'static str {
    match dict {
        Dict::Path => "scmpp_dict_path",
        Dict::Kind => "scmpp_dict_kind",
        Dict::Field => "scmpp_dict_field",
        Dict::Capture => "scmpp_dict_capture",
        Dict::Text => "scmpp_dict_text",
    }
}

/// Rows buffer per table and land `BATCH` at a time; `finish` lands the rest.
pub struct SqliteRows<'c> {
    connection: &'c Connection,
    nodes: Vec<Option<i64>>,
    captures: Vec<Option<i64>>,
    pub written: i64,
}

impl<'c> SqliteRows<'c> {
    pub fn new(connection: &'c Connection) -> Self {
        Self {
            connection,
            nodes: Vec::with_capacity(BATCH * NODE_COLUMNS),
            captures: Vec::with_capacity(BATCH * CAPTURE_COLUMNS),
            written: 0,
        }
    }

    fn land(&mut self, table: &str, columns: usize, cached: bool) -> Result<()> {
        let buffer = if table == "scmpp_node" {
            &mut self.nodes
        } else {
            &mut self.captures
        };
        if buffer.is_empty() {
            return Ok(());
        }
        let count = buffer.len() / columns;
        let sql = insert(table, columns, count);
        if cached {
            self.connection
                .prepare_cached(&sql)?
                .execute(params_from_iter(buffer.iter()))?;
        } else {
            self.connection
                .prepare(&sql)?
                .execute(params_from_iter(buffer.iter()))?;
        }
        buffer.clear();
        self.written += count as i64;
        Ok(())
    }

    /// Lands the partial batches; returns the rows written.
    pub fn finish(mut self) -> Result<i64> {
        self.land("scmpp_node", NODE_COLUMNS, false)?;
        self.land("scmpp_capture", CAPTURE_COLUMNS, false)?;
        Ok(self.written)
    }
}

impl Rows for SqliteRows<'_> {
    fn dict(&mut self, dict: Dict, id: i64, text: &str) -> Result<()> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO {} (id, text) VALUES (?1, ?2)",
                dict_table(dict)
            ))?
            .execute(rusqlite::params![id, text])?;
        self.written += 1;
        Ok(())
    }

    fn node(&mut self, row: &NodeRow) -> Result<()> {
        self.nodes.extend([
            Some(row.file),
            Some(row.pre),
            Some(row.last),
            Some(row.parent),
            Some(row.depth),
            row.sib,
            Some(row.idx),
            Some(row.kind),
            Some(row.field),
            Some(row.start),
            Some(row.end),
            Some(row.named as i64),
        ]);
        if self.nodes.len() == BATCH * NODE_COLUMNS {
            self.land("scmpp_node", NODE_COLUMNS, true)?;
        }
        Ok(())
    }

    fn capture(&mut self, row: &CaptureRow) -> Result<()> {
        self.captures.extend([
            Some(row.file),
            Some(row.pattern),
            Some(row.r#match),
            Some(row.capture),
            Some(row.node),
            Some(row.start),
            Some(row.end),
            row.text,
        ]);
        if self.captures.len() == BATCH * CAPTURE_COLUMNS {
            self.land("scmpp_capture", CAPTURE_COLUMNS, true)?;
        }
        Ok(())
    }
}
