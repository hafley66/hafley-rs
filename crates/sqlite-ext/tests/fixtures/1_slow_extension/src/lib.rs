use sqlite_ext::{
    rusqlite::{functions::FunctionFlags, Connection, Result},
    BulkTrigger, Plugin, RowChange,
};
use std::time::Duration;

struct SlowBatch;

impl BulkTrigger for SlowBatch {
    fn on_batch(&mut self, db: &Connection, batch: &[RowChange]) -> Result<()> {
        sqlite_ext::tracing::info!(rows = batch.len(), "fixture_slow_batch_start");
        std::thread::sleep(Duration::from_millis(25));
        sqlite_ext::statements::exec_cached(
            db,
            "fixture",
            "slow_batches",
            "INSERT INTO slow_batches(rows) VALUES (?1)",
            [batch.len() as i64],
        )?;
        sqlite_ext::tracing::info!("fixture_slow_batch_end");
        Ok(())
    }
}

fn install(db: &Connection) -> Result<()> {
    db.create_scalar_function(
        c"fixture_slow_watch",
        0,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DIRECTONLY,
        |ctx| {
            let db = unsafe { ctx.get_connection()? };
            sqlite_ext::watch(&db, "slow_watch", &["items"], SlowBatch)?;
            Ok(1_i64)
        },
    )
}

const PLUGIN: Plugin = Plugin::new(
    "sqlite_ext_slow_fixture",
    env!("CARGO_PKG_VERSION"),
    "warn",
    install,
);
sqlite_ext::sqlite_extension!(sqlite3_slow_fixture_init, PLUGIN);
