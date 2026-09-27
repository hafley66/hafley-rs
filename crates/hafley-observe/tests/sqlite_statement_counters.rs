#![cfg(feature = "sqlite-sink")]

use hafley_observe::sqlite::{query_plan, StatementFinding, MAX_OPEN_STATEMENTS};
use rusqlite::Connection;
use tracing_capture::{CaptureLayer, SharedStorage};
use tracing_subscriber::prelude::*;

fn seeded_connection() -> Connection {
    let connection = Connection::open_in_memory().expect("in-memory database");
    connection
        .execute_batch(
            "CREATE TABLE arrangement(group_key TEXT, value INTEGER, multiplicity INTEGER);
             CREATE INDEX arrangement_group ON arrangement(group_key, value);
             CREATE TABLE unindexed(value INTEGER);",
        )
        .expect("schema");
    let mut insert = connection
        .prepare("INSERT INTO arrangement VALUES('g', ?1, 1)")
        .expect("prepare");
    for value in 0..200 {
        insert.execute([value]).expect("seed arrangement");
    }
    drop(insert);
    let mut insert = connection
        .prepare("INSERT INTO unindexed VALUES(?1)")
        .expect("prepare");
    for value in 0..200 {
        insert.execute([value]).expect("seed unindexed");
    }
    drop(insert);
    connection
}

fn warnings_for(sql: &str) -> Vec<String> {
    let storage = SharedStorage::default();
    let subscriber = tracing_subscriber::registry().with(CaptureLayer::new(&storage));
    tracing::subscriber::with_default(subscriber, || {
        let connection = seeded_connection();
        hafley_observe::sqlite::instrument(&connection);
        connection
            .prepare(sql)
            .expect("prepare")
            .query_map([], |row| row.get::<_, i64>(0))
            .expect("query")
            .for_each(|row| {
                row.expect("row");
            });
        hafley_observe::sqlite::silence(&connection);
    });
    let storage = storage.lock();
    storage
        .all_events()
        .filter(|event| *event.metadata().level() == tracing::Level::WARN)
        .filter_map(|event| event.message().map(|message| message.to_string()))
        .collect()
}

#[test]
fn an_unindexed_scan_reports_a_table_scan() {
    let warnings = warnings_for("SELECT value FROM unindexed WHERE value > 100");
    assert!(
        warnings
            .iter()
            .any(|w| w == StatementFinding::TableScan.as_str()),
        "expected a table-scan finding, got {warnings:?}"
    );
}

#[test]
fn an_unindexed_order_by_reports_a_temporary_btree_sort() {
    let warnings = warnings_for("SELECT value FROM unindexed ORDER BY value DESC");
    assert!(
        warnings
            .iter()
            .any(|w| w == StatementFinding::TemporaryBtreeSort.as_str()),
        "expected a sort finding, got {warnings:?}"
    );
}

#[test]
fn an_indexed_lookup_reports_nothing() {
    let warnings = warnings_for("SELECT value FROM arrangement WHERE group_key = 'g'");
    assert!(
        warnings.is_empty(),
        "expected no findings, got {warnings:?}"
    );
}

#[test]
fn the_planner_account_names_the_temporary_btree() {
    let connection = seeded_connection();
    let plan = query_plan(
        &connection,
        "SELECT value FROM unindexed ORDER BY value DESC",
    )
    .expect("query plan");
    assert!(
        plan.iter().any(|step| step.contains("TEMP B-TREE")),
        "expected a temp b-tree step, got {plan:?}"
    );
}

#[test]
fn cached_statement_counters_describe_each_execution() {
    let storage = SharedStorage::default();
    let subscriber = tracing_subscriber::registry().with(CaptureLayer::new(&storage));
    tracing::subscriber::with_default(subscriber, || {
        let connection = seeded_connection();
        let sql = "SELECT count(*) FROM unindexed WHERE value > 100";
        // Populate the statement cache before tracing starts.
        let first: i64 = connection.query_row(sql, [], |r| r.get(0)).unwrap();
        assert_eq!(first, 99);
        hafley_observe::sqlite::instrument(&connection);
        for _ in 0..2 {
            let count: i64 = connection.query_row(sql, [], |r| r.get(0)).unwrap();
            assert_eq!(count, 99);
        }
        hafley_observe::sqlite::silence(&connection);
    });
    let storage = storage.lock();
    let profiles = storage
        .all_events()
        .filter(|event| event.metadata().target() == "sqlite")
        .filter(|event| *event.metadata().level() == tracing::Level::DEBUG)
        .filter(|event| event.value("sql").is_some())
        .filter_map(|event| event.value("vm_step").and_then(|v| v.as_int()))
        .collect::<Vec<_>>();
    assert_eq!(profiles.len(), 2, "profile VM steps: {profiles:?}");
    assert_eq!(
        profiles[0], profiles[1],
        "cached executions accumulated VM steps"
    );
    assert!(profiles[0] > 0);
}

#[test]
fn too_many_open_statements_refuse_instrumentation() {
    let storage = SharedStorage::default();
    let subscriber = tracing_subscriber::registry().with(CaptureLayer::new(&storage));
    tracing::subscriber::with_default(subscriber, || {
        let connection = Connection::open_in_memory().expect("in-memory database");
        let statements = (0..=MAX_OPEN_STATEMENTS)
            .map(|_| connection.prepare("SELECT 1").expect("prepare statement"))
            .collect::<Vec<_>>();

        hafley_observe::sqlite::instrument(&connection);
        assert_eq!(
            connection
                .query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
                .expect("query after rejected instrumentation"),
            1
        );
        drop(statements);
    });

    let storage = storage.lock();
    let errors = storage
        .all_events()
        .filter(|event| event.metadata().target() == "sqlite")
        .filter(|event| *event.metadata().level() == tracing::Level::ERROR)
        .filter_map(|event| event.message().map(|message| message.to_string()))
        .collect::<Vec<_>>();
    assert_eq!(
        errors,
        ["refusing sqlite instrumentation: open statement limit exceeded"]
    );
    let traced = storage
        .all_events()
        .filter(|event| event.metadata().target() == "sqlite")
        .filter(|event| *event.metadata().level() != tracing::Level::ERROR)
        .count();
    assert_eq!(traced, 0, "overflow left sqlite tracing enabled");
}
