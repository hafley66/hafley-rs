//! Restore actual interned columns before a historical migration fixture rewinds
//! user_version. Downgrading only the stamp leaves a schema 40 fixture invalid.
use rusqlite::Connection;
pub(crate) fn restore(connection: &Connection) {
    let current: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('agent_turn') WHERE name='role')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    if current {
        connection
            .execute_batch(include_str!("../sql/39_interned_fixture.sql"))
            .unwrap();
    }
}
