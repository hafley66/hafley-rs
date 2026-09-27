//! SQLite allocator and connection memory gauges at a caller-chosen boundary.
//! These counters are separate from process RSS, which includes Rust and any
//! other allocator in the host process. SQLite builds with memory status
//! disabled return zero for the global allocator gauges.

use rusqlite::{ffi, Connection};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SqliteMemory {
    pub allocator_current_bytes: i64,
    pub allocator_peak_bytes: i64,
    pub connection_cache_bytes: i32,
    pub connection_schema_bytes: i32,
    pub connection_statement_bytes: i32,
}

/// `None` means a SQLite status call failed. The allocator gauge is global to
/// the linked SQLite library; the other gauges belong to this connection.
pub fn sample(connection: &Connection) -> Option<SqliteMemory> {
    unsafe {
        let mut allocator_current_bytes = 0;
        let mut allocator_peak_bytes = 0;
        if ffi::sqlite3_status64(
            ffi::SQLITE_STATUS_MEMORY_USED,
            &mut allocator_current_bytes,
            &mut allocator_peak_bytes,
            0,
        ) != ffi::SQLITE_OK
        {
            return None;
        }
        let handle = connection.handle();
        let db_bytes = |status| {
            let mut current = 0;
            let mut peak = 0;
            (ffi::sqlite3_db_status(handle, status, &mut current, &mut peak, 0) == ffi::SQLITE_OK)
                .then_some(current)
        };
        Some(SqliteMemory {
            allocator_current_bytes,
            allocator_peak_bytes,
            connection_cache_bytes: db_bytes(ffi::SQLITE_DBSTATUS_CACHE_USED)?,
            connection_schema_bytes: db_bytes(ffi::SQLITE_DBSTATUS_SCHEMA_USED)?,
            connection_statement_bytes: db_bytes(ffi::SQLITE_DBSTATUS_STMT_USED)?,
        })
    }
}

/// Emit one phase-boundary sample through the caller's existing subscriber.
/// The process peak is deliberately kept separate from SQLite allocator bytes.
pub fn record_memory(connection: &Connection, phase: &str) {
    if !tracing::enabled!(target: "sqlite", tracing::Level::INFO) {
        return;
    }
    let Some(memory) = sample(connection) else {
        tracing::warn!(target: "sqlite", phase, "sqlite memory status unavailable");
        return;
    };
    let process_peak_rss_bytes = crate::rusage::sample().peak_rss_bytes.unwrap_or_default();
    tracing::info!(
        target: "sqlite",
        phase,
        allocator_current_bytes = memory.allocator_current_bytes,
        allocator_peak_bytes = memory.allocator_peak_bytes,
        allocator_status_available = memory.allocator_peak_bytes > 0,
        connection_cache_bytes = memory.connection_cache_bytes,
        connection_schema_bytes = memory.connection_schema_bytes,
        connection_statement_bytes = memory.connection_statement_bytes,
        process_peak_rss_bytes,
        "sqlite memory sample"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_gauges_include_connection_state() -> rusqlite::Result<()> {
        let connection = Connection::open_in_memory()?;
        connection.execute_batch(
            "CREATE TABLE sample(k INTEGER PRIMARY KEY, v TEXT); INSERT INTO sample VALUES(1,'a')",
        )?;
        let statement = connection.prepare("SELECT v FROM sample WHERE k=?1")?;
        let measured = sample(&connection).expect("SQLite memory status");
        assert!(measured.allocator_current_bytes >= 0);
        assert!(measured.allocator_peak_bytes >= measured.allocator_current_bytes);
        assert!(measured.connection_schema_bytes > 0);
        assert!(measured.connection_statement_bytes > 0);
        drop(statement);
        Ok(())
    }
}
