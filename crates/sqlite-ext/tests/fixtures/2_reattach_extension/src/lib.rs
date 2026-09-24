//! The restart probe, loadable: `sqlite_ext_watch(name, tables)` installs a
//! collector and `sqlite_ext_reattach(name, tables)` re-registers it on a
//! freshly loaded connection. `tables` is a comma-separated list. Every
//! delivered batch lands in the ordinary `probe_batches` table, so the sqlite3
//! CLI can drive the whole restart across two processes and read the outcome.

use sqlite_ext::{
    rusqlite::{functions::FunctionFlags, Connection, Result},
    BulkTrigger, Plugin, RowChange,
};

struct ProbeLog;

impl BulkTrigger for ProbeLog {
    fn on_batch(&mut self, db: &Connection, batch: &[RowChange]) -> Result<()> {
        db.execute_batch("CREATE TABLE IF NOT EXISTS probe_batches(rows INTEGER, changes TEXT)")?;
        let mut summary = String::new();
        for change in batch {
            summary.push_str(&format!(
                "{}|{}|{};",
                change.table,
                change.sign.as_integer(),
                change.sequence
            ));
        }
        sqlite_ext::statements::exec_cached(
            db,
            "probe",
            "probe_batches",
            "INSERT INTO probe_batches(rows, changes) VALUES(?1, ?2)",
            sqlite_ext::rusqlite::params![batch.len() as i64, summary],
        )?;
        Ok(())
    }
}

fn install(db: &Connection) -> Result<()> {
    db.create_scalar_function(
        c"sqlite_ext_watch",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DIRECTONLY,
        |ctx| {
            let db = unsafe { ctx.get_connection()? };
            let (name, tables) = arguments(ctx)?;
            let refs: Vec<&str> = tables.iter().map(String::as_str).collect();
            sqlite_ext::watch(&db, &name, &refs, ProbeLog)?;
            Ok(name)
        },
    )?;
    db.create_scalar_function(
        c"sqlite_ext_reattach",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DIRECTONLY,
        |ctx| {
            let db = unsafe { ctx.get_connection()? };
            let (name, tables) = arguments(ctx)?;
            let refs: Vec<&str> = tables.iter().map(String::as_str).collect();
            sqlite_ext::reattach(&db, &name, &refs, ProbeLog)?;
            Ok(name)
        },
    )?;
    Ok(())
}

fn arguments(ctx: &sqlite_ext::rusqlite::functions::Context) -> Result<(String, Vec<String>)> {
    let name: String = ctx.get(0)?;
    let tables: String = ctx.get(1)?;
    let list = tables
        .split(',')
        .map(|table| table.trim().to_owned())
        .collect();
    Ok((name, list))
}

static PLUGIN: Plugin = Plugin::new("sqlite-ext-reattach-fixture", "0.0.0", "warn", install);

sqlite_ext::sqlite_extension!(sqlite3_reattach_fixture_init, PLUGIN);
