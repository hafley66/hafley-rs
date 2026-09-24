use rusqlite::{Connection, Result};

/// Column 0 of the declaration is the only visible one. Hidden columns start
/// after it, and `Inserts` puts the two rowid arguments before column 0.
pub(crate) const SOURCE_COLUMN: usize = 1;
pub(crate) const SIGN_COLUMN: usize = 2;
pub(crate) const FIRST_VALUE_COLUMN: usize = 3;
pub(crate) const ROWID_ARGUMENTS: usize = 2;

pub(crate) fn error(message: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::ModuleError(message.into())
}

pub(crate) fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

pub(crate) fn literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// The collector name doubles as its module name, which SQLite matches as a
/// bare identifier in `USING`.
pub(crate) fn check_identifier(name: &str) -> Result<()> {
    let head = name.chars().next();
    let shaped = head.is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if shaped {
        return Ok(());
    }
    Err(error(format!(
        "collector name {name:?} must be an ASCII identifier"
    )))
}

pub(crate) fn delta_name(collector: &str) -> String {
    format!("{collector}_delta")
}

/// Value columns carry no declared type. SQLite applies column affinity to the
/// arguments it hands xUpdate, and any affinity would rewrite the row's types.
pub(crate) fn declaration(width: usize) -> String {
    let values = (0..width)
        .map(|at| format!(",__value{at} HIDDEN"))
        .collect::<String>();
    format!("CREATE TABLE x(staged_rows INTEGER,__source HIDDEN,__sign HIDDEN{values})")
}

/// The vtab DDL in the exact shape SQLite stores in `sqlite_master`: no schema
/// qualifier.
pub(crate) fn persisted_vtab(name: &str) -> String {
    format!("CREATE VIRTUAL TABLE {} USING {}", quote(name), name)
}

pub(crate) fn vtab_ddl(name: &str) -> String {
    persisted_vtab(name).replacen("CREATE VIRTUAL TABLE ", "CREATE VIRTUAL TABLE main.", 1)
}

pub(crate) fn persisted_delta(collector: &str, width: usize) -> String {
    let values = (0..width)
        .map(|at| format!(",value{at}"))
        .collect::<String>();
    format!(
        "CREATE TABLE {}(sequence INTEGER PRIMARY KEY,source TEXT NOT NULL,\
         sign INTEGER NOT NULL,width INTEGER NOT NULL{values})",
        quote(&delta_name(collector))
    )
}

pub(crate) fn create_delta(collector: &str, width: usize) -> String {
    persisted_delta(collector, width).replacen("CREATE TABLE ", "CREATE TABLE main.", 1)
}

pub(crate) fn insert_delta(collector: &str, width: usize) -> String {
    let names = (0..width)
        .map(|at| format!(",value{at}"))
        .collect::<String>();
    let binds = (0..width + 4)
        .map(|at| format!("?{}", at + 1))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "INSERT INTO main.{}(sequence,source,sign,width{names}) VALUES({binds})",
        quote(&delta_name(collector))
    )
}

pub(crate) fn select_delta(collector: &str, width: usize) -> String {
    let names = (0..width)
        .map(|at| format!(",value{at}"))
        .collect::<String>();
    format!(
        "SELECT sequence,source,sign,width{names} FROM main.{} ORDER BY sequence",
        quote(&delta_name(collector))
    )
}

pub(crate) fn delete_delta(collector: &str) -> String {
    format!("DELETE FROM main.{}", quote(&delta_name(collector)))
}

pub(crate) fn columns(db: &Connection, table: &str) -> Result<Vec<String>> {
    let names = db
        .prepare("SELECT name FROM pragma_table_info(?1) ORDER BY cid")?
        .query_map([table], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>>>()?;
    if names.is_empty() {
        return Err(error(format!("watched table {table:?} has no columns")));
    }
    Ok(names)
}

/// The trigger's stored name, unquoted. Names under the collector prefix are
/// reserved for these three events.
pub(crate) fn trigger_key(collector: &str, table: &str, event: &str) -> String {
    format!("{collector}_{table}_{event}")
}

fn trigger_name(collector: &str, table: &str, event: &str) -> String {
    quote(&trigger_key(collector, table, event))
}

fn body(collector: &str, table: &str, columns: &[String], image: &str, sign: i64) -> String {
    let names = (0..columns.len())
        .map(|at| format!(",__value{at}"))
        .collect::<String>();
    let reads = columns
        .iter()
        .map(|column| format!(",{image}.{}", quote(column)))
        .collect::<String>();
    format!(
        "INSERT INTO {}(__source,__sign{names}) VALUES({},{sign}{reads});",
        quote(collector),
        literal(table)
    )
}

/// AFTER triggers only. A BEFORE trigger would stage a row that a later
/// constraint failure removes from the source.
///
/// `create_triggers` writes these; `persisted_trigger` rebuilds the same
/// statement in the exact shape SQLite stores for reattach to compare against.
pub(crate) fn create_triggers(collector: &str, table: &str, columns: &[String]) -> String {
    ["insert", "delete", "update"]
        .map(|event| {
            format!(
                "{};",
                trigger_stmt(collector, table, event, columns, "main.")
            )
        })
        .join("\n")
}

/// One stored trigger statement: the `main.` qualifier and trailing `;` that
/// SQLite strips from `sqlite_master.sql` are left off.
pub(crate) fn persisted_trigger(
    collector: &str,
    table: &str,
    event: &str,
    columns: &[String],
) -> String {
    trigger_stmt(collector, table, event, columns, "")
}

fn trigger_stmt(
    collector: &str,
    table: &str,
    event: &str,
    columns: &[String],
    qualifier: &str,
) -> String {
    let bodies = match event {
        "insert" => vec![body(collector, table, columns, "NEW", 1)],
        "delete" => vec![body(collector, table, columns, "OLD", -1)],
        _ => vec![
            body(collector, table, columns, "OLD", -1),
            body(collector, table, columns, "NEW", 1),
        ],
    };
    format!(
        "CREATE TRIGGER {qualifier}{} AFTER {} ON {} BEGIN {} END",
        trigger_name(collector, table, event),
        event.to_ascii_uppercase(),
        quote(table),
        bodies.join(" "),
    )
}

pub(crate) fn drop_triggers(collector: &str, tables: &[String]) -> String {
    tables
        .iter()
        .flat_map(|table| {
            ["insert", "delete", "update"]
                .into_iter()
                .map(move |event| {
                    format!(
                        "DROP TRIGGER IF EXISTS main.{};",
                        trigger_name(collector, table, event)
                    )
                })
        })
        .collect()
}
