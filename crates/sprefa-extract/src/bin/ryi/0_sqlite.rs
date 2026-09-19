//! SQLite export only. DDL, columns and wire paths come from TypeSpec.
//! A private staging database is published after commit, with no overwrite.
use std::collections::{HashMap, HashSet};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::sync::Arc;

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub const DDL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/schema/generated/4_facts.sql"
));
pub mod writers {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/schema/generated/7_writers_auto.rs"
    ));
}

pub struct Database {
    connection: Connection,
    temporary: tempfile::NamedTempFile,
    destination: PathBuf,
    pub rows: i64,
    input_path: Option<String>,
    content_id: Option<String>,
    pending: Vec<writers::Fact>,
    pending_bytes: usize,
    max_batch_rows: usize,
}

const BATCH_BYTES: usize = 8 * 1024 * 1024;

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// Every `<x>__start`/`<x>__end` pair on a table with `_content_id`, joined
/// to `line_start` by digest. No `col` column: only offsets are stored.
fn span_lines_view_sql(connection: &Connection) -> Result<String> {
    let mut tables: Vec<String> = connection
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    tables.sort();
    let mut arms = Vec::new();
    for table in &tables {
        if table == "line_start" {
            continue;
        }
        let columns: Vec<String> = connection
            .prepare(&format!("PRAGMA table_info(\"{table}\")"))?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if !columns.iter().any(|column| column == "_content_id") {
            continue;
        }
        for start_column in columns.iter().filter(|column| column.ends_with("__start")) {
            let prefix = &start_column[..start_column.len() - "__start".len()];
            let end_column = format!("{prefix}__end");
            if !columns.contains(&end_column) {
                continue;
            }
            arms.push(format!(
                "SELECT '{table}' AS _table, '{prefix}' AS _span, t.\"_row\" AS _row, \
                 t.\"_content_id\" AS _content_id, t.\"{start_column}\" AS start, \
                 t.\"{end_column}\" AS end FROM \"{table}\" AS t"
            ));
        }
    }
    if arms.is_empty() {
        arms.push(
            "SELECT NULL AS _table, NULL AS _span, NULL AS _row, NULL AS _content_id, \
             NULL AS start, NULL AS end WHERE 0"
                .to_string(),
        );
    }
    Ok(format!(
        "CREATE VIEW \"span_lines\" AS SELECT s._table, s._span, s._row, s._content_id, \
         s.start, s.end, 1 + (SELECT count(*) FROM line_start AS ls, json_each(ls.offsets) AS je \
         WHERE ls.digest = s._content_id AND je.value < s.start) AS line FROM ({}) AS s;",
        arms.join(" UNION ALL ")
    ))
}

impl Database {
    pub fn create(path: &Path) -> Result<Self> {
        if path.as_os_str().is_empty() || path == Path::new(":memory:") {
            return Err("--sqlite requires a filesystem path for a new database".into());
        }
        if std::fs::symlink_metadata(path).is_ok() {
            return Err(format!(
                "--sqlite: {} already exists; supply a new database path",
                path.display()
            )
            .into());
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let temporary = tempfile::Builder::new()
            .prefix(".extract-sqlite-")
            .tempfile_in(parent)?;
        let connection = Connection::open(temporary.path())?;
        connection.set_prepared_statement_cache_capacity(writers::TABLE_COUNT);
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=DELETE; BEGIN IMMEDIATE;",
        )?;
        connection.execute_batch(DDL)?;
        connection.execute_batch(&span_lines_view_sql(&connection)?)?;
        let max_batch_rows = writers::max_batch_rows(&connection)?;
        Ok(Self {
            connection,
            temporary,
            destination: std::path::absolute(path)?,
            rows: 0,
            input_path: None,
            content_id: None,
            pending: Vec::with_capacity(max_batch_rows),
            pending_bytes: 0,
            max_batch_rows,
        })
    }

    pub fn source(&mut self, path: &str, digest: String) -> Result<()> {
        if self.input_path.as_deref() == Some(path) && self.content_id.as_deref() == Some(&digest) {
            return Ok(());
        }
        self.flush_pending()?;
        self.input_path = Some(path.to_owned());
        self.content_id = Some(digest);
        Ok(())
    }

    pub fn clear_source(&mut self) -> Result<()> {
        if self.input_path.is_none() && self.content_id.is_none() {
            return Ok(());
        }
        self.flush_pending()?;
        self.input_path = None;
        self.content_id = None;
        Ok(())
    }

    pub fn insert(&mut self, value: Value) -> Result<()> {
        let encoded = serde_json::to_vec(&value)?;
        self.insert_fact(serde_json::from_slice(&encoded)?, encoded.len())
    }

    pub fn insert_fact(&mut self, fact: writers::Fact, encoded_bytes: usize) -> Result<()> {
        let pending_with_fact = self
            .pending_bytes
            .checked_add(encoded_bytes)
            .ok_or("SQLite batch byte counter overflow")?;
        if !self.pending.is_empty()
            && (self.pending.len() == self.max_batch_rows || pending_with_fact > BATCH_BYTES)
        {
            self.flush_pending()?;
        }
        self.rows = self
            .rows
            .checked_add(1)
            .ok_or("SQLite row counter overflow")?;
        self.pending.push(fact);
        self.pending_bytes = self
            .pending_bytes
            .checked_add(encoded_bytes)
            .ok_or("SQLite batch byte counter overflow")?;
        if self.pending.len() == self.max_batch_rows || self.pending_bytes >= BATCH_BYTES {
            self.flush_pending()?;
        }
        Ok(())
    }

    fn flush_pending(&mut self) -> Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let first_row = self.rows - i64::try_from(self.pending.len())? + 1;
        writers::insert_all(
            &self.connection,
            &writers::Source {
                row: first_row,
                input_path: self.input_path.as_deref(),
                content_id: self.content_id.as_deref(),
            },
            &self.pending,
        )?;
        self.pending.clear();
        self.pending_bytes = 0;
        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        self.flush_pending()?;
        self.connection.execute_batch("COMMIT;")?;
        self.connection.close().map_err(|(_, error)| error)?;
        self.temporary.as_file().sync_all()?;
        self.temporary.persist_noclobber(&self.destination)?;
        let path = shell_quote(&self.destination.to_string_lossy());
        let mut out = std::io::stdout().lock();
        writeln!(
            out,
            "Wrote {} ({} rows)",
            self.destination.display(),
            self.rows
        )?;
        writeln!(out, "Tables: sqlite3 {path} '.tables'")?;
        writeln!(out, "Schema: sqlite3 {path} '.schema'")?;
        writeln!(out, "Query:  sqlite3 -header -column {path} 'SELECT _input_path, family, kind, name FROM node LIMIT 20;'")?;
        writeln!(out, "Lines:  sqlite3 -header -column {path} 'SELECT _table, _row, start, line FROM span_lines LIMIT 20;' (needs --lines for non-empty line_start)")?;
        Ok(())
    }
}

/// The byte count the trail records, taken off the stream rather than
/// re-derived: serde writes straight into the BufWriter and never hands back
/// a length.
struct CountingWriter<W: Write> {
    inner: W,
    bytes: u64,
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.bytes += written as u64;
        Ok(written)
    }

    /// Forwarded rather than left to the default loop, so the BufWriter under
    /// this keeps its one-memcpy path on a 2M-row stream.
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.inner.write_all(buf)?;
        self.bytes += buf.len() as u64;
        Ok(())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
 pub struct Output {
     pub database: Option<Database>,
    stdout: BufWriter<CountingWriter<std::io::Stdout>>,
    /// Line tables by the key a row names its file with: the `path` field in
    /// multi-file streams. Arc so per-row lookups clone a handle, not bytes.
    line_tables: HashMap<String, Arc<Vec<u32>>>,
    /// Every path a lookup already settled, hit or miss, so an unreadable
    /// path costs one probe instead of one per row.
    line_probed: HashSet<String>,
    /// Fallback table for rows that name no file: the per-file verbs' current
    /// input. Whole-project modes leave it unset, so their pathless rows pass
    /// through undecorated.
    line_offsets: Option<Arc<Vec<u32>>>,
    /// Where a row's `path` loads from when its table is not registered:
    /// the root --scip-facts and --family scip read their documents against.
    line_root: Option<PathBuf>,
 }
impl Output {
    pub fn new(path: Option<&Path>) -> Result<Self> {
        Ok(Self {
            database: path.map(Database::create).transpose()?,
            stdout: BufWriter::with_capacity(
                256 * 1024,
                CountingWriter {
                    inner: std::io::stdout(),
                    bytes: 0,
                },
            ),
            line_tables: HashMap::new(),
            line_probed: HashSet::new(),
            line_offsets: None,
            line_root: None,
        })
    }
    /// Scope `--lines` stdout decoration to one input's newline offsets: the
    /// fallback for rows that name no file.
    pub fn set_line_offsets(&mut self, offsets: Vec<u32>) {
        self.line_offsets = Some(Arc::new(offsets));
    }
    /// Register one file's newline offsets under the path its rows will name
    /// (`--resolve` and the diet family see every input's path up front).
    pub fn register_line_table(&mut self, path: &str, offsets: Vec<u32>) {
        self.line_probed.insert(path.to_string());
        self.line_tables.insert(path.to_string(), Arc::new(offsets));
    }
    /// Point `path`-named row lookups at a readable root: --scip-facts and
    /// --family scip name every indexed document, not just supplied paths.
    pub fn set_line_root(&mut self, root: Option<PathBuf>) {
        self.line_root = root;
    }
    /// Bytes written to stdout so far, the trail's write-phase figure.
    pub fn stdout_bytes(&self) -> u64 {
        self.stdout.get_ref().bytes
    }
     pub fn line(&mut self, line: &str) -> Result<()> {
         if let Some(db) = &mut self.database {
             let fact = serde_json::from_slice::<writers::Fact>(line.as_bytes())?;
             return db.insert_fact(fact, line.len());
         }
        self.write_stdout(line.as_bytes())
     }
     pub fn source_fact(
         &mut self,
         path: &str,
         content_id: &sprefa_extract::ContentId,
         fact: &impl Serialize,
     ) -> Result<()> {
         let db = self
             .database
             .as_mut()
             .ok_or("source facts require a SQLite output")?;
         if db.input_path.as_deref() != Some(path) {
             db.source(path, content_id.to_string())?;
         }
         let encoded = serde_json::to_vec(fact)?;
         db.insert_fact(serde_json::from_slice(&encoded)?, encoded.len())
     }
    pub fn clear_source(&mut self) -> Result<()> {
        if let Some(db) = &mut self.database {
            db.clear_source()?;
        }
        Ok(())
    }
    pub fn fact(&mut self, fact: &impl Serialize) -> Result<()> {
        if let Some(db) = &mut self.database {
            let encoded = serde_json::to_vec(fact)?;
            return db.insert_fact(serde_json::from_slice(&encoded)?, encoded.len());
        }
        self.write_stdout(&serde_json::to_vec(fact)?)
    }
    /// The stdout half of both funnel arms. Raw pass-through unless `--lines`
    /// decoration applies, in which case the record is parsed, decorated, and
    /// re-serialized (key order then follows the JSON map, not the struct).
    fn write_stdout(&mut self, encoded: &[u8]) -> Result<()> {
        if !self.line_tables.is_empty()
            || self.line_offsets.is_some()
            || self.line_root.is_some()
        {
            if let Ok(text) = std::str::from_utf8(encoded) {
                // Owned spans suffix their keys (caller_site_start), so the
                // prefilter matches both the plain key and the suffix form.
                if text.contains("\"start\"") || text.contains("_start\":") {
                    let mut value: Value = serde_json::from_str(text)?;
                    if self.decorate_record(&mut value) {
                        serde_json::to_writer(&mut self.stdout, &value)?;
                        self.stdout.write_all(b"\n")?;
                        return Ok(());
                    }
                }
            }
        }
        self.stdout.write_all(encoded)?;
        self.stdout.write_all(b"\n")?;
        Ok(())
     }
    /// Decorate one record against the file it names. A `path` that resolves
    /// to a table wins (data rows also carry a `path`: their JSON dot-path,
    /// which resolves to nothing); otherwise the per-file verbs' current
    /// input applies, and whole-project modes leave that unset so their
    /// unattributable rows stay raw bytes.
    fn decorate_record(&mut self, value: &mut Value) -> bool {
        let owned = self.decorate_owned_spans(value);
        let table = match value.get("path").and_then(Value::as_str) {
            Some(path) => self
                .line_table_for(path)
                .or_else(|| self.line_offsets.clone()),
            None => self.line_offsets.clone(),
        };
        match table {
            Some(offsets) => {
                decorate_lines(value, &offsets);
                true
            }
            None => owned,
        }
    }
    /// A row carrying two files' spans: each span pair names its own file,
    /// so each pair decorates against its own table (resolved_edge's caller
    /// site against caller_path and its callee against callee_path,
    /// resolved_type_edge's owner and target likewise). A pair whose path
    /// resolves to nothing stays raw.
    fn decorate_owned_spans(&mut self, value: &mut Value) -> bool {
        let mut decorated = false;
        for (path_field, start_field, end_field, line_field, col_field) in OWNED_SPANS {
            let start = value.get(start_field).and_then(Value::as_u64);
            let end = value.get(end_field).and_then(Value::as_u64);
            let (Some(start), Some(_)) = (start, end) else {
                continue;
            };
            let Some(path) = value
                .get(path_field)
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                continue;
            };
            let Some(offsets) = self.line_table_for(&path) else {
                continue;
            };
            let Some(object) = value.as_object_mut() else {
                continue;
            };
            let (line, col) = line_col(&offsets, start as u32);
            object.insert(line_field.to_string(), Value::from(line));
            object.insert(col_field.to_string(), Value::from(col));
            decorated = true;
        }
        decorated
    }
    /// The table for one path: registered, else read once from the line root
    /// and cached, or a failed probe remembered so a stream of rows naming an
    /// unreadable file costs one attempt.
    fn line_table_for(&mut self, path: &str) -> Option<Arc<Vec<u32>>> {
        if let Some(table) = self.line_tables.get(path) {
            return Some(Arc::clone(table));
        }
        if self.line_probed.contains(path) {
            return None;
        }
        self.line_probed.insert(path.to_string());
        let full = self.line_root.as_deref()?.join(path);
        let content = std::fs::read(full).ok()?;
        let offsets = Arc::new(sprefa_extract::newline_offsets(&content));
        self.line_tables.insert(path.to_string(), Arc::clone(&offsets));
        Some(offsets)
     }
     pub fn flush(&mut self) -> Result<()> {
         if let Some(db) = &mut self.database {
             db.flush_pending()?;
         }
         self.stdout.flush()?;
         Ok(())
     }
     pub fn finish(mut self) -> Result<()> {
         self.stdout.flush()?;
         if let Some(db) = self.database {
             db.finish()?;
         }
         Ok(())
     }
 }

/// Span pairs whose owning file is another field of the same row: the pair
/// decorates against that field's table, one decoration per owning path.
const OWNED_SPANS: [(&str, &str, &str, &str, &str); 4] = [
    (
        "caller_path",
        "caller_site_start",
        "caller_site_end",
        "caller_site_line",
        "caller_site_col",
    ),
    (
        "callee_path",
        "callee_start",
        "callee_end",
        "callee_line",
        "callee_col",
    ),
    (
        "owner_path",
        "owner_start",
        "owner_end",
        "owner_line",
        "owner_col",
    ),
    (
        "target_path",
        "target_start",
        "target_end",
        "target_line",
        "target_col",
    ),
];

/// 1-based (line, col) of a byte against newline offsets. Col counts BYTES
/// from the line start, not characters, matching the spans it decorates.
fn line_col(offsets: &[u32], start: u32) -> (u32, u32) {
    let line = offsets.partition_point(|offset| *offset < start);
    let line_start = line.checked_sub(1).map_or(0, |index| offsets[index] + 1);
    ((line + 1) as u32, start - line_start + 1)
}

/// Add `line` and `col` beside every `start`/`end` pair at any depth, so a
/// decorated record is the undecorated bytes plus exactly those fields.
fn decorate_lines(value: &mut Value, offsets: &[u32]) {
    match value {
        Value::Object(map) => {
            let start = map.get("start").and_then(Value::as_u64);
            let end = map.get("end").and_then(Value::as_u64);
            if let (Some(start), Some(_end)) = (start, end) {
                let (line, col) = line_col(offsets, start as u32);
                map.insert("line".to_string(), Value::from(line));
                map.insert("col".to_string(), Value::from(col));
            }
            for child in map.values_mut() {
                decorate_lines(child, offsets);
            }
        }
        Value::Array(items) => {
            for item in items {
                decorate_lines(item, offsets);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn protocol(version: u32) -> writers::Fact {
        serde_json::from_value(serde_json::json!({"record": "protocol", "version": version}))
            .unwrap()
    }
    fn stored_rows(database: &Database) -> i64 {
        database
            .connection
            .query_row("SELECT count(*) FROM protocol", [], |row| row.get(0))
            .unwrap()
    }
    #[test]
    fn byte_budget_accounting_flushes_without_large_allocations() {
        // Declared encoded sizes exercise accounting; facts stay small.
        let directory = tempfile::tempdir().unwrap();
        let mut database = Database::create(&directory.path().join("facts.db")).unwrap();
        database.insert_fact(protocol(1), 7).unwrap();
        assert_eq!((database.pending.len(), database.pending_bytes), (1, 7));
        assert_eq!(stored_rows(&database), 0);

        database.insert_fact(protocol(2), BATCH_BYTES + 1).unwrap();
        assert_eq!((database.pending.len(), database.pending_bytes), (0, 0));
        assert_eq!(stored_rows(&database), 2);

        let sub_cap = BATCH_BYTES / 2 + 1;
        database.insert_fact(protocol(3), sub_cap).unwrap();
        assert_eq!(
            (database.pending.len(), database.pending_bytes),
            (1, sub_cap)
        );
        assert_eq!(stored_rows(&database), 2);

        database.insert_fact(protocol(4), sub_cap).unwrap();
        assert_eq!(
            (database.pending.len(), database.pending_bytes),
            (1, sub_cap)
        );
        assert_eq!(stored_rows(&database), 3);

        database.flush_pending().unwrap();
        assert_eq!((database.pending.len(), database.pending_bytes), (0, 0));
        assert_eq!(stored_rows(&database), 4);
    }
}
