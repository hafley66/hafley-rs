use rusqlite::{types::Value, Connection, Result};
use sqlite_ext::{reattach, watch, BulkTrigger, Counts, RowChange, Sign, Watch};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
struct Recorder {
    batches: Arc<Mutex<Vec<Vec<RowChange>>>>,
}

impl BulkTrigger for Recorder {
    fn on_batch(&mut self, _db: &Connection, batch: &[RowChange]) -> Result<()> {
        self.batches.lock().unwrap().push(batch.to_vec());
        Ok(())
    }
}

impl Recorder {
    fn batches(&self) -> Vec<Vec<RowChange>> {
        self.batches.lock().unwrap().clone()
    }
}

/// A file-backed database, the restart surface an in-memory connection cannot
/// exercise. Reopening the path is the second SQLite connection.
fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "sqlite-ext-reattach-{name}-{}.sqlite",
        std::process::id()
    ));
    std::fs::remove_file(&path).ok();
    path
}

fn reopen(path: &PathBuf) -> Result<Connection> {
    Connection::open(path)
}

fn summary(batch: &[RowChange]) -> Vec<(String, Sign, Vec<i64>)> {
    batch
        .iter()
        .map(|change| {
            (
                change.table.clone(),
                change.sign,
                change
                    .values
                    .iter()
                    .map(|value| match value {
                        Value::Integer(int) => *int,
                        other => panic!("unexpected value {other:?}"),
                    })
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn a_reopened_connection_collects_again_after_reattach() -> Result<()> {
    let path = scratch("reopen");
    let first = Recorder::default();
    {
        let db = Connection::open(&path)?;
        db.execute_batch("CREATE TABLE src(id INTEGER PRIMARY KEY, v INTEGER)")?;
        watch(&db, "p", &["src"], first.clone())?;
        db.execute_batch("BEGIN; INSERT INTO src VALUES(1,10),(2,20); COMMIT;")?;
        assert_eq!(first.batches().len(), 1);
    }

    let second = Recorder::default();
    let db = reopen(&path)?;
    reattach(&db, "p", &["src"], second.clone())?;

    // Without the reattach this prepare answers `no such module: p`; with it,
    // the persisted triggers connect and fire exactly once per transaction.
    db.execute_batch(
        "BEGIN;
         INSERT INTO src VALUES(3,30);
         UPDATE src SET v = 99 WHERE id = 1;
         DELETE FROM src WHERE id = 2;
         COMMIT;",
    )?;
    let batches = second.batches();
    assert_eq!(batches.len(), 1, "one callback per transaction");
    assert_eq!(
        summary(&batches[0]),
        vec![
            ("src".into(), Sign::Insert, vec![3, 30]),
            ("src".into(), Sign::Delete, vec![1, 10]),
            ("src".into(), Sign::Insert, vec![1, 99]),
            ("src".into(), Sign::Delete, vec![2, 20]),
        ]
    );
    assert_eq!(
        batches[0].iter().map(|c| c.sequence).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    // The persisted collector table reads, and the drain left nothing staged.
    assert_eq!(
        db.query_row("SELECT staged_rows FROM p", [], |r| r.get::<_, i64>(0))?,
        0
    );
    Ok(())
}

#[test]
fn a_reattach_and_a_watch_refuse_each_other() -> Result<()> {
    let path = scratch("conflict");
    let db = Connection::open(&path)?;
    db.execute_batch("CREATE TABLE src(id INTEGER PRIMARY KEY, v INTEGER)")?;
    watch(&db, "p", &["src"], Recorder::default())?;

    let err = reattach(&db, "p", &["src"], Recorder::default()).unwrap_err();
    assert!(err.to_string().contains("already installed"), "{err}");

    let err = watch(&db, "p", &["src"], Recorder::default()).unwrap_err();
    assert!(err.to_string().contains("already installed"), "{err}");
    Ok(())
}

#[test]
fn a_second_reattach_on_the_reopened_connection_is_refused() -> Result<()> {
    let path = scratch("twice");
    {
        let db = Connection::open(&path)?;
        db.execute_batch("CREATE TABLE src(id INTEGER PRIMARY KEY, v INTEGER)")?;
        watch(&db, "p", &["src"], Recorder::default())?;
    }
    let db = reopen(&path)?;
    reattach(&db, "p", &["src"], Recorder::default())?;
    let err = reattach(&db, "p", &["src"], Recorder::default()).unwrap_err();
    assert!(err.to_string().contains("already installed"), "{err}");
    Ok(())
}

#[test]
fn a_database_without_the_collector_is_refused() -> Result<()> {
    let path = scratch("absent");
    let db = Connection::open(&path)?;
    db.execute_batch("CREATE TABLE src(id INTEGER PRIMARY KEY, v INTEGER)")?;

    let err = reattach(&db, "p", &["src"], Recorder::default()).unwrap_err();
    assert!(err.to_string().contains("not persisted"), "{err}");
    assert_eq!(sqlite_ext::counts(&db, "p"), None, "no half registration");
    Ok(())
}

/// The live column list is rebuilt into the expected trigger text, so a column
/// added after the first process closed no longer matches what is persisted.
#[test]
fn a_drifted_source_schema_is_refused_without_leaving_state() -> Result<()> {
    let path = scratch("drift");
    {
        let db = Connection::open(&path)?;
        db.execute_batch("CREATE TABLE src(id INTEGER PRIMARY KEY, v INTEGER)")?;
        watch(&db, "p", &["src"], Recorder::default())?;
    }
    {
        let db = Connection::open(&path)?;
        db.execute_batch("ALTER TABLE src ADD COLUMN extra INTEGER")?;
    }
    let db = reopen(&path)?;
    let err = reattach(&db, "p", &["src"], Recorder::default()).unwrap_err();
    assert!(err.to_string().contains("does not match"), "{err}");
    assert_eq!(sqlite_ext::counts(&db, "p"), None);
    // Nothing was registered, so a source write still fails the way the
    // restart probe measured it, not through a half-attached collector.
    let err = db
        .execute_batch("INSERT INTO src VALUES(4,40,4)")
        .unwrap_err();
    assert!(err.to_string().contains("no such module"), "{err}");
    Ok(())
}

#[test]
fn a_missing_persisted_object_is_named_by_the_refusal() -> Result<()> {
    let path = scratch("missing-trigger");
    {
        let db = Connection::open(&path)?;
        db.execute_batch("CREATE TABLE src(id INTEGER PRIMARY KEY, v INTEGER)")?;
        watch(&db, "p", &["src"], Recorder::default())?;
        db.execute_batch("DROP TRIGGER main.p_src_update")?;
    }
    let db = reopen(&path)?;
    let err = reattach(&db, "p", &["src"], Recorder::default()).unwrap_err();
    assert!(
        err.to_string().contains("p_src_update"),
        "the refusal names the missing object: {err}"
    );
    Ok(())
}

#[test]
fn a_rolled_back_transaction_after_a_reattach_draws_no_batch() -> Result<()> {
    let path = scratch("rollback");
    {
        let db = Connection::open(&path)?;
        db.execute_batch("CREATE TABLE src(id INTEGER PRIMARY KEY, v INTEGER)")?;
        watch(&db, "p", &["src"], Recorder::default())?;
    }
    let recorder = Recorder::default();
    let db = reopen(&path)?;
    reattach(&db, "p", &["src"], recorder.clone())?;
    db.execute_batch("BEGIN; INSERT INTO src VALUES(1,10); ROLLBACK;")?;
    assert_eq!(recorder.batches(), Vec::<Vec<RowChange>>::new());
    db.execute_batch("INSERT INTO src VALUES(2,20)")?;
    assert_eq!(recorder.batches().len(), 1);
    assert_eq!(
        summary(&recorder.batches()[0]),
        vec![("src".into(), Sign::Insert, vec![2, 20])]
    );
    Ok(())
}

#[test]
fn savepoints_keep_their_meaning_after_a_reattach() -> Result<()> {
    let path = scratch("savepoint");
    {
        let db = Connection::open(&path)?;
        db.execute_batch("CREATE TABLE src(id INTEGER PRIMARY KEY, v INTEGER)")?;
        watch(&db, "p", &["src"], Recorder::default())?;
    }
    let recorder = Recorder::default();
    let db = reopen(&path)?;
    reattach(&db, "p", &["src"], recorder.clone())?;

    db.execute_batch(
        "BEGIN;
         INSERT INTO src VALUES(1,10);
         SAVEPOINT kept;
         INSERT INTO src VALUES(2,20);
         ROLLBACK TO kept;
         RELEASE kept;
         COMMIT;",
    )?;
    let batches = recorder.batches();
    assert_eq!(batches.len(), 1);
    assert_eq!(
        summary(&batches[0]),
        vec![("src".into(), Sign::Insert, vec![1, 10])]
    );
    Ok(())
}

/// The persisted shadow table outlives the process; rows past the reattach's
/// own memory cap spill into it and come back in sequence order at the drain.
#[test]
fn a_spill_after_a_reattach_returns_the_persisted_shadow_rows_in_order() -> Result<()> {
    let path = scratch("spill");
    {
        let db = Connection::open(&path)?;
        db.execute_batch("CREATE TABLE src(id INTEGER PRIMARY KEY, v INTEGER)")?;
        watch(&db, "p", &["src"], Recorder::default())?;
    }
    let recorder = Recorder::default();
    let db = reopen(&path)?;
    Watch::new("p")
        .tables(&["src"])
        .staged_rows(2)
        .reattach(&db, recorder.clone())?;
    db.execute_batch("BEGIN; INSERT INTO src VALUES(1,10),(2,20),(3,30),(4,40),(5,50); COMMIT;")?;
    let batches = recorder.batches();
    assert_eq!(batches.len(), 1);
    assert_eq!(
        summary(&batches[0]),
        vec![
            ("src".into(), Sign::Insert, vec![1, 10]),
            ("src".into(), Sign::Insert, vec![2, 20]),
            ("src".into(), Sign::Insert, vec![3, 30]),
            ("src".into(), Sign::Insert, vec![4, 40]),
            ("src".into(), Sign::Insert, vec![5, 50]),
        ]
    );
    assert_eq!(
        batches[0].iter().map(|c| c.sequence).collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4],
        "memory rows and persisted shadow rows interleave by sequence"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM p_delta", [], |r| r.get::<_, i64>(0))?,
        0,
        "the drain emptied the persisted shadow table"
    );
    Ok(())
}

#[test]
fn tables_of_different_arities_reattach_at_the_widest() -> Result<()> {
    let path = scratch("mixed");
    {
        let db = Connection::open(&path)?;
        db.execute_batch(
            "CREATE TABLE narrow(id INTEGER PRIMARY KEY, v INTEGER);
             CREATE TABLE wide(id INTEGER PRIMARY KEY, x INTEGER, y INTEGER);",
        )?;
        watch(&db, "p", &["narrow", "wide"], Recorder::default())?;
    }
    let recorder = Recorder::default();
    let db = reopen(&path)?;
    reattach(&db, "p", &["narrow", "wide"], recorder.clone())?;
    db.execute_batch(
        "BEGIN; INSERT INTO narrow VALUES(1,10); INSERT INTO wide VALUES(2,20,30); COMMIT;",
    )?;
    let batches = recorder.batches();
    assert_eq!(batches.len(), 1);
    assert_eq!(
        summary(&batches[0]),
        vec![
            ("narrow".into(), Sign::Insert, vec![1, 10]),
            ("wide".into(), Sign::Insert, vec![2, 20, 30]),
        ]
    );
    Ok(())
}

#[test]
fn a_reattach_starts_from_zeroed_counts() -> Result<()> {
    let path = scratch("counts");
    {
        let db = Connection::open(&path)?;
        db.execute_batch("CREATE TABLE src(id INTEGER PRIMARY KEY, v INTEGER)")?;
        watch(&db, "p", &["src"], Recorder::default())?;
        db.execute_batch("INSERT INTO src VALUES(1,10)")?;
    }
    let db = reopen(&path)?;
    reattach(&db, "p", &["src"], Recorder::default())?;
    assert_eq!(sqlite_ext::counts(&db, "p"), Some(Counts::default()));
    db.execute_batch("INSERT INTO src VALUES(2,20)")?;
    let counts = sqlite_ext::counts(&db, "p").unwrap();
    assert_eq!(counts.update, 1);
    assert_eq!(counts.sync, 1);
    Ok(())
}
