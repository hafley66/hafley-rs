//! The convoy rail. `instant` spawns one `boop db` per read; 512 of them ran
//! at once on 2026-08-20 and each ran its own full transcript sync, so the
//! machine served 512 identical passes on one SQLite writer lock.
//!
//! Two receipts. The COUNT is the contract: a caller that finds the sync lock
//! held records `deferred` and performs no pass of its own. The WALL is the
//! incident: N concurrent invocations finish inside a budget. The count cannot
//! express the second, because the thing that went wrong was N passes taking
//! the writer lock in turn, and the store records no row for a pass that
//! wrote nothing.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Transcripts the convoy fixture seeds. The machine's own store held 3893
/// when this was measured, and the pre-fix cost is per candidate, so a smaller
/// fixture measures process startup instead of the defect.
const CONVOY_SESSIONS: usize = 4000;

/// Transcripts the two single-invocation fixtures seed. Neither one measures
/// per-candidate cost, so they stay cheap.
const SESSIONS: usize = 200;

/// Concurrent invocations. The incident was 512; a test that spawns 512
/// processes IS the thing this rail forbids, and 24 already serialises every
/// pass against the same lock.
const CONCURRENT: usize = 24;

/// Wall for `CONCURRENT` invocations. Measured on this machine at
/// `CONVOY_SESSIONS`: 2.81s at 898be94, 0.20s after. The count assertions
/// above it are the contract; this is the backstop that reads the incident
/// itself, and no count expresses it, because a pass that wrote nothing
/// leaves no row behind.
const CONVOY_BUDGET: std::time::Duration = std::time::Duration::from_millis(1500);

/// Wall for one warm invocation against an already-synced store.
const WARM_BUDGET: std::time::Duration = std::time::Duration::from_secs(2);

struct Fixture {
    home: PathBuf,
    sessions: usize,
}

impl Fixture {
    fn new(name: &str, sessions: usize) -> Fixture {
        let home = std::env::temp_dir().join(format!("boop-convoy-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("make temp home");
        for index in 0..sessions {
            write_transcript(&home, index);
        }
        Fixture { home, sessions }
    }

    /// Move the transcript root, which is what a live harness does between any
    /// two reads. A quiet fixture is not the machine the incident happened on.
    fn touch_root(&self) {
        let root = self.home.join(".claude").join("projects");
        let marker = root.join(".touch");
        std::fs::write(&marker, "").expect("move the transcript root");
        std::fs::remove_file(&marker).expect("move the transcript root");
    }

    fn db(&self) -> PathBuf {
        self.home.join("boop.db")
    }

    fn trail(&self) -> PathBuf {
        self.home.join("sync-trail.ndjson")
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_boop"));
        command
            .args([
                "db",
                "SELECT COUNT(*) AS n FROM agent_turn",
                "--format",
                "ndjson",
            ])
            .env("HOME", &self.home)
            .env("BOOP_DB", self.db())
            .env("BOOP_SYNC_TRAIL", self.trail())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        command
    }

    /// One invocation, start to finish, returning the turn count it printed.
    fn read(&self) -> i64 {
        let output = self.command().output().expect("run boop db");
        assert!(
            output.status.success(),
            "boop db failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).expect("utf8 stdout");
        let line = text.lines().last().unwrap_or_default().to_owned();
        serde_json::from_str::<serde_json::Value>(&line)
            .unwrap_or_else(|error| panic!("parse {line:?}: {error}"))["n"]
            .as_i64()
            .expect("n is an integer")
    }

    /// How many `start` and `deferred` records the sync trail holds. Read as
    /// plain NDJSON rather than through the library, so this file compiles
    /// against a build that has no sync trail at all and fails on its
    /// assertions instead of on its imports.
    fn trail_counts(&self) -> (usize, usize) {
        let text = std::fs::read_to_string(self.trail()).unwrap_or_default();
        let records: Vec<serde_json::Value> = text
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        let kind = |wanted: &str| {
            records
                .iter()
                .filter(|record| record["kind"] == wanted)
                .count()
        };
        (kind("start"), kind("deferred"))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn write_transcript(home: &Path, index: usize) {
    let session = format!("{index:08}-0000-0000-0000-000000000000");
    let dir = home
        .join(".claude")
        .join("projects")
        .join(format!("-tmp-convoy-{index}"));
    std::fs::create_dir_all(&dir).expect("make claude project dir");
    let body = format!(
        "{{\"type\":\"user\",\"uuid\":\"{session}-u\",\"sessionId\":\"{session}\",\
         \"timestamp\":\"2026-08-20T10:00:00.000Z\",\"cwd\":\"/tmp/convoy\",\
         \"message\":{{\"role\":\"user\",\"content\":\"ask\"}}}}\n\
         {{\"type\":\"assistant\",\"uuid\":\"{session}-a\",\"parentUuid\":\"{session}-u\",\
         \"sessionId\":\"{session}\",\"timestamp\":\"2026-08-20T10:00:01.000Z\",\
         \"cwd\":\"/tmp/convoy\",\"message\":{{\"role\":\"assistant\",\
         \"content\":[{{\"type\":\"text\",\"text\":\"answer\"}}]}}}}\n"
    );
    std::fs::write(dir.join(format!("{session}.jsonl")), body).expect("write transcript");
}

/// RECEIPT (boop-db-convoy): failed pre-fix. At 898be94 the held lock meant
/// nothing, the invocation ran its own full pass, and the trail did not exist.
#[test]
fn a_caller_that_finds_the_sync_lock_held_reads_without_syncing() {
    let fixture = Fixture::new("held", SESSIONS);
    let seeded = fixture.read();
    assert_eq!(
        seeded,
        fixture.sessions as i64 * 2,
        "the first pass projects the fixture"
    );
    let (starts_before, _) = fixture.trail_counts();

    // The test process itself is the pass in flight. flock is per descriptor,
    // so a lock this process holds is a lock the child cannot take.
    let lock_path = PathBuf::from(format!("{}.sync.lock", fixture.db().display()));
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .expect("open the sync lock");
    lock.lock().expect("hold the sync lock");

    let read = fixture.read();
    let (starts_after, deferred) = fixture.trail_counts();

    assert_eq!(
        read, seeded,
        "a deferred caller still answers from the store"
    );
    assert_eq!(
        starts_after, starts_before,
        "a caller that found the lock held must perform no pass of its own"
    );
    assert_eq!(deferred, 1, "and must record why it did not");

    lock.unlock().expect("release the sync lock");
}

/// RECEIPT (boop-db-convoy): failed pre-fix. At 898be94 24 concurrent
/// invocations ran 24 full passes; the transcript is in
/// `TASKS/boop-db-convoy.REPORT.md`.
#[test]
fn concurrent_reads_perform_one_sync_pass_between_them() {
    let fixture = Fixture::new("convoy", CONVOY_SESSIONS);
    let seeded = fixture.read();
    assert_eq!(seeded, fixture.sessions as i64 * 2);
    // A fresh trail, so the counts below cover only the concurrent burst.
    std::fs::write(fixture.trail(), "").expect("truncate the trail");
    fixture.touch_root();

    let started = std::time::Instant::now();
    let children: Vec<_> = (0..CONCURRENT)
        .map(|_| fixture.command().spawn().expect("spawn boop db"))
        .collect();
    for mut child in children {
        let status = child.wait().expect("wait for boop db");
        assert!(status.success(), "a concurrent boop db failed: {status}");
    }
    let wall = started.elapsed();

    // The wall is asserted first so a build with no sync trail still fails on
    // the incident rather than on the absence of the receipt.
    assert!(
        wall < CONVOY_BUDGET,
        "{CONCURRENT} concurrent reads took {wall:?}, over the {CONVOY_BUDGET:?} budget"
    );
    let (starts, deferred) = fixture.trail_counts();
    assert_eq!(
        starts + deferred,
        CONCURRENT,
        "every invocation records what it did with the sync"
    );
    assert!(
        starts < CONCURRENT,
        "{CONCURRENT} invocations ran {starts} passes: single-flight did not hold"
    );
}

/// RECEIPT (boop-db-convoy): the warm read is the one `instant` makes per
/// view. Pre-fix it re-ran the full pass including 3893 no-op cursor writes.
#[test]
fn a_warm_read_stays_under_its_budget() {
    let fixture = Fixture::new("warm", SESSIONS);
    fixture.read();
    fixture.touch_root();
    let started = std::time::Instant::now();
    fixture.read();
    let wall = started.elapsed();
    assert!(
        wall < WARM_BUDGET,
        "a warm read took {wall:?}, over the {WARM_BUDGET:?} budget"
    );
}
