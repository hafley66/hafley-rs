//! scm++ storage: capture and CST rows per file, then the compiled SQL over them.
use std::collections::HashSet;

use hafley_scm::scmpp::{Compiled, ROOT};
use rusqlite::functions::FunctionFlags;
use serde_json::{Map, Value};
use tree_sitter::{QueryCursor, StreamingIterator, Tree};

use super::sqlite::writers::{models, Fact};
use super::sqlite::Database;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const INDEXES: &str = "\
CREATE INDEX IF NOT EXISTS scmpp_capture_match ON capture(_input_path, pattern, \"match\", capture);
CREATE INDEX IF NOT EXISTS scmpp_capture_node ON capture(pattern, capture, _content_id, start, \"end\", kind);
CREATE INDEX IF NOT EXISTS scmpp_edge_to ON edge(_content_id, to__start, to__end, to_kind);
CREATE INDEX IF NOT EXISTS scmpp_edge_from ON edge(_content_id, from__start, from__end, from_kind);";

fn span(node: tree_sitter::Node) -> models::SpanOut {
    models::SpanOut { start: node.start_byte() as u32, end: node.end_byte() as u32 }
}

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
    let mut ordinal = 0u32;
    for pattern in &compiled.patterns {
        let names = pattern.query.capture_names();
        let root = names.iter().position(|name| *name == ROOT).expect("flat pattern root") as u32;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&pattern.query, tree.root_node(), src);
        while let Some(found) = matches.next() {
            let Some(anchor) = found.captures().iter().find(|capture| capture.index == root) else {
                continue;
            };
            let whole = span(anchor.node);
            for capture in found.captures() {
                let _row = tracing::trace_span!("scmpp_capture_row").entered();
                let node = capture.node;
                let name = names[capture.index as usize];
                let text = if capture.index == root {
                    String::new()
                } else {
                    String::from_utf8_lossy(&src[node.byte_range()]).into_owned()
                };
                let bytes = text.len() + 64;
                let row = models::Capture {
                    query: query.to_string(),
                    capture: name.to_string(),
                    text,
                    start: node.start_byte() as u32,
                    end: node.end_byte() as u32,
                    match_start: whole.start,
                    match_end: whole.end,
                    pattern: Some(pattern.id as u32),
                    r#match: Some(ordinal),
                    kind: Some(node.kind().to_string()),
                };
                db.insert_fact(Fact::Capture(row), bytes)?;
            }
            ordinal += 1;
        }
        drop(matches);
        if cursor.did_exceed_match_limit() {
            return Err(format!("{path}: tree-sitter match limit exceeded in scm++ level {}", pattern.id).into());
        }
    }
    if cst_written.insert(content_id.to_string()) {
        write_cst(db, tree)?;
    }
    Ok(())
}

/// Every node, named and anonymous; each non-root node gets the edge from its parent.
fn write_cst(db: &mut Database, tree: &Tree) -> Result<()> {
    let language = tree.language();
    let mut failed = None;
    hafley_scm::cst::walk_streaming(tree, |node, parent: Option<(u32, u32, u16)>, slot| {
        let _row = tracing::trace_span!("scmpp_cst_row").entered();
        let here = span(node);
        let kind = node.kind();
        let mut write = || -> Result<()> {
            db.insert_fact(
                Fact::Node(models::Node {
                    fact: None,
                    family: models::FamilyTag::Cst,
                    span: here.clone(),
                    kind: kind.to_string(),
                    name: None,
                    named: Some(node.is_named()),
                }),
                96,
            )?;
            if let Some((start, end, from_kind)) = parent {
                db.insert_fact(
                    Fact::Edge(models::Edge {
                        fact: None,
                        family: models::FamilyTag::Cst,
                        kind: "child".into(),
                        from: models::SpanOut { start, end },
                        from_kind: language.node_kind_for_id(from_kind).map(str::to_string),
                        to: here.clone(),
                        to_kind: Some(kind.to_string()),
                        field: slot.field.and_then(|id| language.field_name_for_id(id)).map(str::to_string),
                        index: Some(slot.index),
                        named_index: slot.named_index,
                    }),
                    128,
                )?;
            }
            Ok(())
        };
        if failed.is_none() {
            failed = write().err();
        }
        (here.start, here.end, node.kind_id())
    });
    failed.map_or(Ok(()), Err)
}

/// Runs the SQL into `scmpp_row` and returns its rows in order, one JSON object each.
pub fn run_sql(db: &mut Database, compiled: &Compiled) -> Result<Vec<Map<String, Value>>> {
    db.flush()?;
    let connection = db.connection();
    connection.execute_batch(INDEXES)?;
    connection.create_scalar_function(
        "regexp",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |context| {
            let pattern = context.get_or_create_aux(0, |value| -> std::result::Result<regex::Regex, Box<dyn std::error::Error + Send + Sync>> {
                Ok(regex::Regex::new(value.as_str()?)?)
            })?;
            let text = context.get::<Option<String>>(1)?;
            Ok(text.is_some_and(|text| pattern.is_match(&text)))
        },
    )?;
    connection.execute_batch(&format!("DROP TABLE IF EXISTS scmpp_row; CREATE TABLE scmpp_row AS {};", compiled.sql))?;
    let mut statement = connection.prepare("SELECT * FROM scmpp_row ORDER BY rowid")?;
    let columns: Vec<String> = statement.column_names().into_iter().map(str::to_string).collect();
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
