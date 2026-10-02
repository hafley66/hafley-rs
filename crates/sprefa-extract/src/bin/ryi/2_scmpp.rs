//! scm++ storage: capture and CST rows per file, then the compiled SQL over them.
use std::collections::HashSet;

use hafley_scm::scmpp::{CaptureRow, Compiled, CstRow};
use serde_json::{Map, Value};
use tree_sitter::Tree;

use super::sqlite::writers::{models, Fact};
use super::sqlite::Database;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// One file: a capture row per capture of every flat pattern (`match` = per-file ordinal),
/// then node/edge rows (family cst) once per content id.
pub fn write_file(
    db: &mut Database,
    compiled: &Compiled,
    query: &str,
    path: &str,
    content_id: &str,
    src: &[u8],
    tree: &Tree,
    cst_written: &mut HashSet<String>,
) -> Result<()> {
    db.source(path, content_id.to_string())?;
    hafley_scm::scmpp::capture_rows(compiled, tree, src, |row: CaptureRow| -> Result<()> {
        let _row = tracing::trace_span!("scmpp_capture_row").entered();
        let bytes = row.text.len() + 64;
        let capture = models::Capture {
            query: query.to_string(),
            capture: row.capture.to_string(),
            text: row.text,
            start: row.start,
            end: row.end,
            match_start: row.match_start,
            match_end: row.match_end,
            pattern: Some(row.pattern as u32),
            r#match: Some(row.r#match),
            kind: Some(row.kind.to_string()),
        };
        db.insert_fact(Fact::Capture(capture), bytes)
    })
    .map_err(|error| format!("{path}: {error}"))?;
    if cst_written.insert(content_id.to_string()) {
        hafley_scm::scmpp::cst_rows(tree, |row: CstRow| write_cst_row(db, row))?;
    }
    Ok(())
}

/// Every node, named and anonymous; each non-root node gets the edge from its parent.
fn write_cst_row(db: &mut Database, row: CstRow) -> Result<()> {
    let _row = tracing::trace_span!("scmpp_cst_row").entered();
    let here = models::SpanOut {
        start: row.start,
        end: row.end,
    };
    db.insert_fact(
        Fact::Node(models::Node {
            fact: None,
            family: models::FamilyTag::Cst,
            span: here.clone(),
            kind: row.kind.to_string(),
            name: None,
            named: Some(row.named),
        }),
        96,
    )?;
    let Some((start, end, from_kind)) = row.parent else {
        return Ok(());
    };
    db.insert_fact(
        Fact::Edge(models::Edge {
            fact: None,
            family: models::FamilyTag::Cst,
            kind: "child".into(),
            from: models::SpanOut { start, end },
            from_kind: Some(from_kind.to_string()),
            to: here,
            to_kind: Some(row.kind.to_string()),
            field: row.field.map(str::to_string),
            index: Some(row.index),
            named_index: row.named_index,
        }),
        128,
    )
}

/// Runs the SQL into `scmpp_row` and returns its rows in order, one JSON object each.
pub fn run_sql(db: &mut Database, compiled: &Compiled) -> Result<Vec<Map<String, Value>>> {
    db.flush()?;
    let connection = db.connection();
    connection.execute_batch(hafley_scm::scmpp::INDEXES)?;
    hafley_scm::scmpp::register_regexp(connection)?;
    connection.execute_batch(&format!(
        "DROP TABLE IF EXISTS scmpp_row; CREATE TABLE scmpp_row AS {};",
        compiled.sql
    ))?;
    let mut statement = connection.prepare("SELECT * FROM scmpp_row ORDER BY rowid")?;
    let columns: Vec<String> = statement
        .column_names()
        .into_iter()
        .map(str::to_string)
        .collect();
    let mut rows = statement.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let _row = tracing::trace_span!("scmpp_row").entered();
        let mut object = Map::new();
        for (index, column) in columns.iter().enumerate() {
            let value = match row.get_ref(index)? {
                rusqlite::types::ValueRef::Null => Value::Null,
                rusqlite::types::ValueRef::Integer(number) => number.into(),
                rusqlite::types::ValueRef::Real(number) => number.into(),
                rusqlite::types::ValueRef::Text(text) => String::from_utf8_lossy(text).into(),
                rusqlite::types::ValueRef::Blob(bytes) => String::from_utf8_lossy(bytes).into(),
            };
            object.insert(column.clone(), value);
        }
        out.push(object);
    }
    Ok(out)
}
