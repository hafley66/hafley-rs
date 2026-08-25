//! Twelve `boop` processes writing one mailbox at once. The question this
//! answers is whether moving the bus into sqlite trades an append-only file
//! for a contention storm.
//!
//! The parent re-execs this same test binary once per worker. Each worker
//! appends `ROWS_EACH` envelopes (row plus first transition, one transaction)
//! and acks half of them, then writes its own timings to a file the parent
//! folds.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use boop_store::bus::{self, Message};
use boop_store::Store;

const WORKERS: usize = 12;
const ROWS_EACH: usize = 50;
const ACKS_EACH: usize = 25;
/// A single append that waits this long is the defect the receipt looks for.
const APPEND_BUDGET_MS: u128 = 1000;

const WORKER_ENV: &str = "BOOP_MAIL_CONTENTION_WORKER";
const DB_ENV: &str = "BOOP_MAIL_CONTENTION_DB";
const TEST_NAME: &str = "twelve_processes_share_one_mailbox_without_a_storm";

fn row(worker: usize, index: usize) -> Message {
    Message {
        id: format!("m-contend-{worker}-{index}"),
        from: format!("contender-{worker}"),
        to: "receipt-coordinator".to_owned(),
        from_timestamp: bus::now_iso(),
        to_timestamp: None,
        kind: "note".to_owned(),
        reply_to: None,
        body: format!("worker {worker} row {index}"),
        r#ref: None,
        rc: None,
        detail: None,
    }
}

/// One worker: append then ack, recording the slowest single append.
fn run_worker(worker: usize, db: &Path) {
    let store = Store::open(db.to_path_buf()).expect("open the mailbox");
    let started = Instant::now();
    let mut slowest_ms = 0u128;
    for index in 0..ROWS_EACH {
        let append = Instant::now();
        bus::insert_message(&store, "bus", &row(worker, index), "mailbox")
            .unwrap_or_else(|error| panic!("worker {worker} row {index}: {error:#}"));
        slowest_ms = slowest_ms.max(append.elapsed().as_millis());
    }
    let ids: Vec<String> = (0..ACKS_EACH)
        .map(|index| format!("m-contend-{worker}-{index}"))
        .collect();
    let ack = Instant::now();
    let stamped = bus::ack_messages(&store, &ids, &bus::now_iso())
        .unwrap_or_else(|error| panic!("worker {worker} ack: {error:#}"));
    assert_eq!(stamped, ACKS_EACH, "every ack stamps an open row");
    let report = db.with_file_name(format!("worker-{worker}.txt"));
    std::fs::write(
        report,
        format!(
            "{slowest_ms} {} {}",
            ack.elapsed().as_millis(),
            started.elapsed().as_millis()
        ),
    )
    .expect("write the worker report");
}

/// RECEIPT. Twelve processes append 600 envelopes and ack 300 against one
/// database. Every row lands, every row carries a transition, no worker fails
/// on a busy database, and the slowest single append is reported.
#[test]
fn twelve_processes_share_one_mailbox_without_a_storm() {
    if let Ok(worker) = std::env::var(WORKER_ENV) {
        let db = PathBuf::from(std::env::var(DB_ENV).expect("the worker db path"));
        run_worker(worker.parse().expect("a worker index"), &db);
        return;
    }

    let root = std::env::temp_dir().join(format!("boop-mail-contention-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let db = root.join("boop.db");
    Store::open(db.clone()).expect("create the mailbox");

    let started = Instant::now();
    let mut running = Vec::new();
    for worker in 0..WORKERS {
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST_NAME, "--nocapture"])
            .env(WORKER_ENV, worker.to_string())
            .env(DB_ENV, &db)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn a worker");
        running.push((worker, child));
    }
    let children: Vec<_> = running
        .into_iter()
        .map(|(worker, child)| (worker, child.wait_with_output().expect("await a worker")))
        .collect();
    for (worker, output) in &children {
        assert!(
            output.status.success(),
            "worker {worker} failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let wall = started.elapsed();

    let mut slowest_append_ms = 0u128;
    let mut slowest_ack_ms = 0u128;
    let mut slowest_worker_ms = 0u128;
    for worker in 0..WORKERS {
        let text = std::fs::read_to_string(root.join(format!("worker-{worker}.txt")))
            .unwrap_or_else(|error| panic!("worker {worker} left no report: {error}"));
        let mut parts = text.split_whitespace();
        let append: u128 = parts.next().unwrap().parse().unwrap();
        let ack: u128 = parts.next().unwrap().parse().unwrap();
        let total: u128 = parts.next().unwrap().parse().unwrap();
        slowest_append_ms = slowest_append_ms.max(append);
        slowest_ack_ms = slowest_ack_ms.max(ack);
        slowest_worker_ms = slowest_worker_ms.max(total);
    }

    let store = Store::open(db.clone()).unwrap();
    let rows = bus::messages_in(&store).unwrap();
    assert_eq!(rows.len(), WORKERS * ROWS_EACH, "every append lands");
    assert_eq!(
        rows.iter().filter(|row| row.to_timestamp.is_some()).count(),
        WORKERS * ACKS_EACH,
        "every ack lands"
    );
    let orphans: i64 = store
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM agent_mail m
             WHERE NOT EXISTS (SELECT 1 FROM agent_delivery_transition t
                                WHERE t.message_id = m.message_id)",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(orphans, 0, "no row exists without a transition");

    println!(
        "{WORKERS} processes x {ROWS_EACH} appends + {ACKS_EACH} acks: \
         wall {:.3}s, slowest worker {slowest_worker_ms}ms, \
         slowest single append {slowest_append_ms}ms, slowest ack batch {slowest_ack_ms}ms",
        wall.as_secs_f64()
    );
    assert!(
        slowest_append_ms < APPEND_BUDGET_MS,
        "an append waited {slowest_append_ms}ms, past the {APPEND_BUDGET_MS}ms budget"
    );
    let _ = std::fs::remove_dir_all(&root);
}
