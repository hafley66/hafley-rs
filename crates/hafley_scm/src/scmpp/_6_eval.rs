//! SQLite side of scm++: the `regexp()` a cross-level `#match?` lowers to, the read indexes, and
//! the `build` path's per-file evaluation of `match_sql` in `:memory:`.
use std::collections::{HashMap, HashSet};

use rusqlite::functions::FunctionFlags;
use rusqlite::{params, Connection};
use tree_sitter::Tree;

use super::_0_types::{Compiled, MatchKey, ScmppError};
use super::_2_compile::ROOT;
use super::_5_rows::{capture_rows, cst_rows};

pub const INDEXES: &str = "\
CREATE INDEX IF NOT EXISTS scmpp_capture_match ON capture(_input_path, pattern, \"match\", capture);
CREATE INDEX IF NOT EXISTS scmpp_capture_node ON capture(pattern, capture, _content_id, start, \"end\", kind);
CREATE INDEX IF NOT EXISTS scmpp_edge_to ON edge(_content_id, to__start, to__end, to_kind);
CREATE INDEX IF NOT EXISTS scmpp_edge_from ON edge(_content_id, from__start, from__end, from_kind);";

/// Only the columns the lowering reads; `crates/sprefa-extract/schema/1_facts.tsp` owns the full tables.
const TABLES: &str = "\
CREATE TABLE capture(_input_path TEXT, _content_id TEXT, pattern INTEGER, \"match\" INTEGER,
  capture TEXT, text TEXT, start INTEGER, \"end\" INTEGER, kind TEXT);
CREATE TABLE edge(_content_id TEXT, family TEXT, from__start INTEGER, from__end INTEGER, from_kind TEXT,
  to__start INTEGER, to__end INTEGER, to_kind TEXT, field TEXT, \"index\" INTEGER, named_index INTEGER);";

pub fn register_regexp(connection: &Connection) -> rusqlite::Result<()> {
    connection.create_scalar_function(
        "regexp",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |context| {
            let pattern = context.get_or_create_aux(
                0,
                |value| -> Result<regex::Regex, Box<dyn std::error::Error + Send + Sync>> {
                    Ok(regex::Regex::new(value.as_str()?)?)
                },
            )?;
            let text = context.get::<Option<String>>(1)?;
            Ok(text.is_some_and(|text| pattern.is_match(&text)))
        },
    )
}

impl From<rusqlite::Error> for ScmppError {
    fn from(error: rusqlite::Error) -> Self {
        ScmppError::Sql(error.to_string())
    }
}

/// The level-0 matches of one file that `match_sql` keeps.
pub fn accepted(
    compiled: &Compiled,
    tree: &Tree,
    src: &[u8],
) -> Result<HashSet<MatchKey>, ScmppError> {
    let connection = Connection::open_in_memory()?;
    connection.execute_batch(TABLES)?;
    register_regexp(&connection)?;
    let level0 = compiled.plan.pattern;
    let mut keys = HashMap::<u32, MatchKey>::new();
    {
        let transaction = connection.unchecked_transaction()?;
        let mut capture = transaction
            .prepare("INSERT INTO capture VALUES ('', '', ?1, ?2, ?3, ?4, ?5, ?6, ?7)")?;
        capture_rows(compiled, tree, src, |row| {
            if row.pattern == level0 && row.capture != ROOT {
                keys.entry(row.r#match)
                    .or_default()
                    .push((row.capture.into(), row.start, row.end));
            }
            capture.execute(params![
                row.pattern,
                row.r#match,
                row.capture,
                row.text,
                row.start,
                row.end,
                row.kind
            ])?;
            Ok::<_, ScmppError>(())
        })?;
        let mut edge = transaction
            .prepare("INSERT INTO edge VALUES ('', 'cst', ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)")?;
        cst_rows(tree, |row| {
            let Some((start, end, kind)) = row.parent else {
                return Ok(());
            };
            edge.execute(params![
                start,
                end,
                kind,
                row.start,
                row.end,
                row.kind,
                row.field,
                row.index,
                row.named_index
            ])?;
            Ok::<_, ScmppError>(())
        })?;
        drop((capture, edge));
        transaction.commit()?;
    }
    connection.execute_batch(INDEXES)?;
    let mut statement = connection.prepare(&compiled.match_sql)?;
    let ordinals = statement.query_map([], |row| row.get::<_, u32>(0))?;
    let mut out = HashSet::new();
    for ordinal in ordinals {
        let mut key = keys.remove(&ordinal?).unwrap_or_default();
        key.sort_unstable();
        out.insert(key);
    }
    Ok(out)
}
