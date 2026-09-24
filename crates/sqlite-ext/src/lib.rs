//! Shared rusqlite extension callbacks and transactional row collection.
//!
//! @comment-ok: crate documentation, and the example below is a compiled doctest.
//!
//! SQLite fires triggers per row. A consumer that runs work inside that landing
//! pays for it once per row. This crate collects the rows into a virtual table,
//! the one SQLite object that receives `xSavepoint`, `xRollbackTo` and a
//! write-capable `xSync`, and hands the whole transaction to
//! [`BulkTrigger::on_batch`] once at `xSync`, before SQLite commits.
//!
//! ```
//! use rusqlite::Connection;
//! use sqlite_ext::{watch, BulkTrigger, RowChange};
//!
//! struct Count(usize);
//! impl BulkTrigger for Count {
//!     fn on_batch(&mut self, _db: &Connection, batch: &[RowChange]) -> rusqlite::Result<()> {
//!         self.0 += batch.len();
//!         Ok(())
//!     }
//! }
//!
//! let db = Connection::open_in_memory()?;
//! db.execute_batch("CREATE TABLE orders(id INTEGER PRIMARY KEY, amount INTEGER)")?;
//! watch(&db, "orders_collector", &["orders"], Count(0))?;
//! db.execute_batch("INSERT INTO orders VALUES(1,10),(2,20)")?;
//! # Ok::<(), rusqlite::Error>(())
//! ```
//!
//! After a process restart, [`reattach`] re-registers a collector over the
//! schema a previous connection persisted, so a fresh connection collects
//! again without recreating anything.

mod collector;
#[path = "0_module.rs"]
mod module;
#[path = "2_plugin.rs"]
mod plugin;
mod schema;
#[path = "1_statements.rs"]
pub mod statements;
mod vtab;

pub use collector::{BulkTrigger, Collector, Counts, RowChange, Sign, STAGED_BYTES, STAGED_ROWS};
pub use module::{vtab_callback, VtabCallbacks};
pub use plugin::Plugin;
pub use rusqlite;
pub use tracing;
pub use vtab::{counts, reattach, watch, Watch};
