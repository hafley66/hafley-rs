//! scm++ run: the interned store in the run's SQLite database, then the compiled SQL over it.
use hafley_scm::scmpp::Compiled;
use rusqlite::functions::FunctionFlags;
use rusqlite::Connection;
use serde_json::{Map, Value};
use tree_sitter::Tree;

use super::sqlite::Database;

#[path = "1a_scmpp_rows.rs"]
pub mod rows;
#[path = "1b_scmpp_sqlite.rs"]
pub mod store_sqlite;

pub use rows::Store;
use rows::Result;
use store_sqlite::SqliteRows;

/// The scm++ tables in `db`, and the run's dictionaries.
pub fn open(db: &mut Database) -> Result<Store> {
    db.flush()?;
    db.connection().execute_batch(store_sqlite::DDL)?;
    Ok(Store::default())
}

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

/// One file's capture rows, and its node rows when the plan reads nodes.
pub fn write_file(
    db: &mut Database,
    store: &mut Store,
    compiled: &Compiled,
    path: &str,
    src: &[u8],
    tree: &Tree,
) -> Result<()> {
    db.flush()?;
    let mut rows = SqliteRows::new(db.connection());
    store
        .write_file(&mut rows, compiled, path, src, tree)
        .map_err(|error| format!("{path}: {error}"))?;
    let written = rows.finish()?;
    db.rows += written;
    Ok(())
}

/// Runs the SQL into `scmpp_row` and returns its rows in order, one JSON object each.
pub fn run_sql(db: &mut Database, compiled: &Compiled) -> Result<Vec<Map<String, Value>>> {
    db.flush()?;
    let connection = db.connection();
    connection.execute_batch(store_sqlite::INDEXES)?;
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
