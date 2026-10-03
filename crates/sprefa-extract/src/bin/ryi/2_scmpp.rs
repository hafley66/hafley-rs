//! scm++ storage: capture and CST rows per file, then the compiled SQL over them.
use std::collections::HashSet;

use hafley_scm::scmpp::{Compiled, ROOT};
use rusqlite::functions::FunctionFlags;
use rusqlite::Connection;
use serde_json::{Map, Value};
use tree_sitter::{QueryCursor, StreamingIterator, Tree};

use super::sqlite::writers::{models, Fact};
use super::sqlite::Database;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub const INDEXES: &str = "\
CREATE INDEX IF NOT EXISTS scmpp_capture_match ON capture(_input_path, pattern, \"match\", capture);
CREATE INDEX IF NOT EXISTS scmpp_capture_node ON capture(pattern, capture, _content_id, start, \"end\", kind);
CREATE INDEX IF NOT EXISTS scmpp_edge_to ON edge(_content_id, to__start, to__end, to_kind);
CREATE INDEX IF NOT EXISTS scmpp_edge_from ON edge(_content_id, from__start, from__end, from_kind);";

/// The `regexp()` a cross-level `#match?` lowers to.
pub fn register_regexp(connection: &Connection) -> rusqlite::Result<()> {
    connection.create_scalar_function(
        "regexp",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |context| {
            let pattern =
                context.get_or_create_aux(
                    0,
                    |value| -> std::result::Result<
                        regex::Regex,
                        Box<dyn std::error::Error + Send + Sync>,
                    > { Ok(regex::Regex::new(value.as_str()?)?) },
                )?;
            let text = context.get::<Option<String>>(1)?;
            Ok(text.is_some_and(|text| pattern.is_match(&text)))
        },
    )
}

/// Prepares the compiled SQL on the store before any row lands, so a query past SQLite's
/// join width or expression depth fails before a file is read.
pub fn check_sql(db: &mut Database, compiled: &Compiled) -> Result<()> {
    fn depth(level: &hafley_scm::scmpp::Level) -> usize {
        let nested = level.rels.iter().flat_map(|rel| rel.target.iter().chain(rel.stop.iter()));
        1 + nested.map(|inner| depth(inner)).max().unwrap_or(0)
    }
    db.flush()?;
    let connection = db.connection();
    register_regexp(connection)?;
    if let Err(error) = connection.prepare(&compiled.sql) {
        let captures: usize = compiled.patterns.iter().map(|pattern| pattern.captures.len()).sum();
        return Err(format!(
            "scm++ SQL for {captures} captures, {} levels deep, exceeds a SQLite limit: {error}",
            depth(&compiled.plan)
        )
        .into());
    }
    Ok(())
}

pub struct CaptureRow<'q> {
    pub pattern: u16,
    /// Per-file ordinal across every flat pattern.
    pub r#match: u32,
    pub capture: &'q str,
    /// Empty for the `@__root` capture.
    pub text: String,
    pub start: u32,
    pub end: u32,
    pub match_start: u32,
    pub match_end: u32,
    pub kind: &'q str,
}

/// The `capture` rows of every flat pattern over one file.
pub fn capture_rows<'q>(
    compiled: &'q Compiled,
    tree: &'q Tree,
    src: &[u8],
    mut row: impl FnMut(CaptureRow<'q>) -> Result<()>,
) -> Result<()> {
    let mut ordinal = 0u32;
    for pattern in &compiled.patterns {
        let names = pattern.query.capture_names();
        let root = names
            .iter()
            .position(|name| *name == ROOT)
            .expect("flat pattern root") as u32;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&pattern.query, tree.root_node(), src);
        while let Some(found) = matches.next() {
            let Some(anchor) = found
                .captures()
                .iter()
                .find(|capture| capture.index == root)
            else {
                continue;
            };
            for capture in found.captures() {
                let node = capture.node;
                let text = if capture.index == root {
                    String::new()
                } else {
                    String::from_utf8_lossy(&src[node.byte_range()]).into_owned()
                };
                row(CaptureRow {
                    pattern: pattern.id,
                    r#match: ordinal,
                    capture: names[capture.index as usize],
                    text,
                    start: node.start_byte() as u32,
                    end: node.end_byte() as u32,
                    match_start: anchor.node.start_byte() as u32,
                    match_end: anchor.node.end_byte() as u32,
                    kind: node.kind(),
                })?;
            }
            ordinal += 1;
        }
        drop(matches);
        if cursor.did_exceed_match_limit() {
            return Err(format!(
                "scm++ level {}: tree-sitter match limit exceeded",
                pattern.id
            )
            .into());
        }
    }
    Ok(())
}

pub struct CstRow<'a> {
    pub start: u32,
    pub end: u32,
    pub kind: &'a str,
    pub named: bool,
    /// `(start, end, kind)` of the parent; `None` at the root, which has no edge.
    pub parent: Option<(u32, u32, &'a str)>,
    pub field: Option<&'a str>,
    pub index: u32,
    pub named_index: Option<u32>,
}

/// Every node, named and anonymous, in preorder; the first error stops the writes.
pub fn cst_rows(tree: &Tree, mut row: impl FnMut(CstRow<'_>) -> Result<()>) -> Result<()> {
    let language = tree.language();
    let fields: Vec<Option<&str>> = (0..=language.field_count() as u16)
        .map(|id| language.field_name_for_id(id))
        .collect();
    let mut failed = None;
    hafley_scm::cst::walk_streaming(tree, |node, parent: Option<(u32, u32, &str)>, slot| {
        let here = (
            node.start_byte() as u32,
            node.end_byte() as u32,
            node.kind(),
        );
        if failed.is_none() {
            failed = row(CstRow {
                start: here.0,
                end: here.1,
                kind: here.2,
                named: node.is_named(),
                parent,
                field: slot.field.and_then(|id| fields[id as usize]),
                index: slot.index,
                named_index: slot.named_index,
            })
            .err();
        }
        here
    });
    failed.map_or(Ok(()), Err)
}

/// One file: a capture row per capture of every flat pattern (`match` = per-file ordinal),
/// then node/edge rows (family cst) once per content id when the plan has a relation.
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
    capture_rows(compiled, tree, src, |row: CaptureRow| -> Result<()> {
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
    if reads_cst(compiled) && cst_written.insert(content_id.to_string()) {
        cst_rows(tree, |row: CstRow| write_cst_row(db, row))?;
    }
    Ok(())
}

/// The SQL reads `edge` only through a relation; nested levels hang off the root's
/// relations, so a root without one means a plan without one.
pub fn reads_cst(compiled: &Compiled) -> bool {
    !compiled.plan.rels.is_empty()
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
    connection.execute_batch(INDEXES)?;
    register_regexp(connection)?;
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
