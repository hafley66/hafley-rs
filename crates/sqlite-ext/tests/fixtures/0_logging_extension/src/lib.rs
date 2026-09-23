use sqlite_ext::{
    rusqlite::{functions::FunctionFlags, Connection, Result},
    BulkTrigger, Plugin, RowChange,
};

struct LogBatch;

impl BulkTrigger for LogBatch {
    fn on_batch(&mut self, db: &Connection, batch: &[RowChange]) -> Result<()> {
        sqlite_ext::tracing::info!(rows = batch.len(), "fixture_log_batch");
        sqlite_ext::statements::exec_cached(
            db,
            "fixture",
            "log_batches",
            "INSERT INTO log_batches(rows) VALUES (?1)",
            [batch.len() as i64],
        )?;
        Ok(())
    }
}

fn install(db: &Connection) -> Result<()> {
    db.create_scalar_function(
        c"fixture_log_watch",
        0,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DIRECTONLY,
        |ctx| {
            let db = unsafe { ctx.get_connection()? };
            sqlite_ext::watch(&db, "log_watch", &["items"], LogBatch)?;
            Ok(1_i64)
        },
    )
}

const PLUGIN: Plugin = Plugin::new(
    "sqlite_ext_log_fixture",
    env!("CARGO_PKG_VERSION"),
    "warn",
    install,
);
sqlite_ext::sqlite_extension!(sqlite3_log_fixture_init, PLUGIN);
