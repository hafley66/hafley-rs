use rusqlite::{Connection, Result};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

fn fixture(name: &str) -> PathBuf {
    // A lane that exports CARGO_TARGET_DIR sends the fixture build's output
    // there, not to tests/fixtures/target where the plain path would look.
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/target"));
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let built = Command::new("cargo")
        .args([
            "build",
            "--offline",
            "--locked",
            "--quiet",
            "--manifest-path",
        ])
        .arg(root.join("Cargo.toml"))
        .output()
        .expect("cargo builds the fixture extensions");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let suffix = if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    target
        .join("debug")
        .join(format!("libsqlite_ext_{name}_fixture.{suffix}"))
}

#[test]
fn two_loadable_plugins_share_one_connection_and_transaction() -> Result<()> {
    let log = fixture("log");
    let slow = fixture("slow");
    let db = Connection::open_in_memory()?;
    db.execute_batch(
        "CREATE TABLE items(id INTEGER PRIMARY KEY, value INTEGER NOT NULL);
         CREATE TABLE log_batches(rows INTEGER NOT NULL);
         CREATE TABLE slow_batches(rows INTEGER NOT NULL);",
    )?;
    unsafe {
        db.load_extension_enable()?;
        db.load_extension(&log, Some("sqlite3_log_fixture_init"))?;
        db.load_extension(&slow, Some("sqlite3_slow_fixture_init"))?;
        db.load_extension_disable()?;
    }
    assert_eq!(
        db.query_row("SELECT fixture_log_watch()", [], |r| r.get::<_, i64>(0))?,
        1
    );
    assert_eq!(
        db.query_row("SELECT fixture_slow_watch()", [], |r| r.get::<_, i64>(0))?,
        1
    );

    let started = Instant::now();
    db.execute_batch("BEGIN; INSERT INTO items VALUES(1,10),(2,20); COMMIT;")?;
    assert!(started.elapsed() >= Duration::from_millis(25));
    db.execute_batch(
        "BEGIN;
         INSERT INTO items VALUES(3,30),(4,40);
         SAVEPOINT discarded;
         INSERT INTO items VALUES(5,50);
         ROLLBACK TO discarded;
         RELEASE discarded;
         COMMIT;",
    )?;
    db.execute_batch("BEGIN; INSERT INTO items VALUES(6,60); ROLLBACK;")?;

    let read = |table: &str| -> Result<Vec<i64>> {
        let mut statement = db.prepare(&format!("SELECT rows FROM {table} ORDER BY rowid"))?;
        let rows = statement.query_map([], |r| r.get(0))?.collect();
        rows
    };
    assert_eq!(read("log_batches")?, [2, 2]);
    assert_eq!(read("slow_batches")?, [2, 2]);
    assert_eq!(
        db.query_row("SELECT count(*) FROM items", [], |r| r.get::<_, i64>(0))?,
        4
    );
    Ok(())
}

#[test]
fn both_native_plugins_emit_observe_events() {
    let output = Command::new(std::env::current_exe().expect("test executable"))
        .args([
            "--exact",
            "two_loadable_plugins_share_one_connection_and_transaction",
            "--nocapture",
        ])
        .env("HAFLEY_LOG_FORMAT", "json")
        .env(
            "RUST_LOG",
            "sqlite_ext=trace,sqlite_ext_log_fixture=trace,sqlite_ext_slow_fixture=trace",
        )
        .output()
        .expect("run the native plugin test with observation enabled");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let events = String::from_utf8(output.stderr).expect("UTF-8 trace");
    for name in [
        "fixture_log_batch",
        "fixture_slow_batch_start",
        "collector_drain",
    ] {
        assert!(events.contains(name), "missing {name} in native trace");
    }
}

/// The restart gate over a loadable plugin: the second `Connection` starts
/// with nothing registered, so without `sqlite_ext_reattach` its first source
/// write answers `no such module`.
#[test]
fn a_loadable_plugin_reattaches_a_reopened_collector() -> Result<()> {
    let ext = fixture("reattach");
    let path = std::env::temp_dir().join(format!(
        "sqlite-ext-reattach-fixture-{}.sqlite",
        std::process::id()
    ));
    std::fs::remove_file(&path).ok();
    let load = |db: &Connection| -> Result<()> {
        unsafe {
            db.load_extension_enable()?;
            db.load_extension(&ext, Some("sqlite3_reattach_fixture_init"))?;
            db.load_extension_disable()?;
        }
        Ok(())
    };

    {
        let first = Connection::open(&path)?;
        load(&first)?;
        first.execute_batch(
            "CREATE TABLE src(id INTEGER PRIMARY KEY, v INTEGER);
             SELECT sqlite_ext_watch('p','src');
             INSERT INTO src VALUES(1,10),(2,20);",
        )?;
    }
    let second = Connection::open(&path)?;
    load(&second)?;
    second.execute_batch(
        "SELECT sqlite_ext_reattach('p','src');
         INSERT INTO src VALUES(3,30);
         UPDATE src SET v = 99 WHERE id = 1;
         DELETE FROM src WHERE id = 2;",
    )?;
    let batches: Vec<(i64, String)> = second
        .prepare("SELECT rows, changes FROM probe_batches ORDER BY rowid")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>>>()?;
    // The first row is the closed process's committed batch; the reopened
    // collector's own sequence restarts at 0.
    assert_eq!(
        batches,
        vec![
            (2, "src|1|0;src|1|1;".into()),
            (1, "src|1|0;".into()),
            (2, "src|-1|0;src|1|1;".into()),
            (1, "src|-1|0;".into()),
        ]
    );
    std::fs::remove_file(&path).ok();
    Ok(())
}
