//! FAIL-PRE-FIX: with `journal_mode=delete` the second of three concurrent
//! writers died on `database is locked` past the busy timeout. WAL plus the
//! busy timeout (`ident.rs`) is what makes all three land.
//!
//! `BOOP_WAL_DB=<path>` runs the same battery against an existing store; point
//! it at a COPY of `~/.agent/boop.db`, never at the live file.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use boop::{Store, TraceEvent};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

const WRITERS: usize = 3;
const ROWS_EACH: usize = 20;

fn store_path() -> (PathBuf, Option<PathBuf>) {
    if let Ok(path) = std::env::var("BOOP_WAL_DB") {
        return (PathBuf::from(path), None);
    }
    let root = std::env::temp_dir().join(format!(
        "boop-wal-writers-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    (root.join("boop.db"), Some(root))
}

fn event(lane: &str, index: usize) -> TraceEvent {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    TraceEvent {
        event_key: format!("wal-{lane}-{index}"),
        lane: lane.to_owned(),
        trace: Some("trace-wal-writers".to_owned()),
        session: None,
        kind: "note".to_owned(),
        from_lane: None,
        to_lane: None,
        started_ts: Some(now),
        finished_ts: Some(now),
        delivery_state: None,
        classification: None,
        detail: format!("three-writer contention row {index}"),
        created_ts: now,
    }
}

/// RECEIPT. Three connections write the store at once and every row lands;
/// the store is in WAL, which is what keeps the losers waiting instead of
/// failing. Reports the slowest writer's wall time.
#[test]
fn three_concurrent_writers_all_land() {
    let (db, root) = store_path();
    let mode: String = {
        let store = Store::open(db.clone()).unwrap();
        let (_, rows) = store.passthrough("PRAGMA journal_mode").unwrap();
        rows[0]
            .as_object()
            .and_then(|row| row.values().next())
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_owned()
    };
    assert_eq!(mode, "wal", "the store must be in WAL: {}", db.display());

    let started = Instant::now();
    let mut writers = Vec::new();
    for writer in 0..WRITERS {
        let db = db.clone();
        writers.push(std::thread::spawn(move || {
            let lane = format!("wal-writer-{}-{writer}", std::process::id());
            let store = Store::open(db).unwrap();
            let own = Instant::now();
            for index in 0..ROWS_EACH {
                store
                    .record_trace_event(&event(&lane, index))
                    .unwrap_or_else(|error| panic!("writer {writer} row {index}: {error:#}"));
            }
            own.elapsed()
        }));
    }
    let slowest = writers
        .into_iter()
        .map(|writer| writer.join().unwrap())
        .max()
        .unwrap();
    println!(
        "{WRITERS} writers x {ROWS_EACH} rows: slowest writer {:.3}s, battery {:.3}s, db {}",
        slowest.as_secs_f64(),
        started.elapsed().as_secs_f64(),
        db.display()
    );

    let store = Store::open(db).unwrap();
    let (_, rows) = store
        .passthrough(&format!(
            "SELECT COUNT(*) AS n FROM agent_trace_event WHERE event_key LIKE 'wal-wal-writer-{}-%'",
            std::process::id()
        ))
        .unwrap();
    let landed = rows[0]
        .as_object()
        .and_then(|row| row.get("n"))
        .and_then(|value| value.as_i64())
        .unwrap_or_default();
    assert_eq!(landed as usize, WRITERS * ROWS_EACH, "every row lands");
    if let Some(root) = root {
        let _ = std::fs::remove_dir_all(root);
    }
}
