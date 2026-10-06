//! The whole SQLite concern over fixture tables: typed catalog round-trips,
//! JSONL column-for-column matches, publication atomicity, batch ceilings and
//! the byte budget, pinned by one whole-output snapshot.

#![cfg(feature = "cli")]
#![allow(dead_code)]

#[path = "../src/bin/ryi/0_sqlite.rs"]
pub(crate) mod sqlite;

#[test]
fn whole_output() {
    crate::fixture_runner::run("sqlite", crate::sqlite_support::evaluate);
}
