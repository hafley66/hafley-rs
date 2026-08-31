//! FAIL-PRE-FIX: a lane that burned its whole retry budget and then stopped
//! without completing told its parent nothing; the only row was the completion
//! rc, read after the fact. On the pre-fix tree every count below is 0.

use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::Duration;

use boop_acp::channel::{Delivery, LaneChannel, TurnEvent, TurnReceipt};
use boop_proc::supervise::{LaneRun, EXITED_WITHOUT_COMPLETION, RETRYING, RETRY_BUDGET_EXHAUSTED};

/// One temp HOME and store for this whole binary, so the mood lookup inside a
/// lane run never opens the machine's own store.
fn root() -> PathBuf {
    static ONCE: Once = Once::new();
    let root = std::env::temp_dir().join(format!("boop-failure-hail-{}", std::process::id()));
    ONCE.call_once(|| {
        std::fs::create_dir_all(root.join("home")).unwrap();
        std::env::set_var("HOME", root.join("home"));
        std::env::set_var("BOOP_DB", root.join("boop.db"));
    });
    root
}

fn mail_dir(tag: &str) -> PathBuf {
    let dir = root().join(tag);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A harness whose every turn dies on a provider flake.
#[derive(Default)]
struct FlakingChannel {
    turns: usize,
}

impl LaneChannel for FlakingChannel {
    fn conversation_id(&self) -> Option<String> {
        Some("thread-1".to_owned())
    }
    fn start_turn(&mut self, _text: &str) -> anyhow::Result<()> {
        self.turns += 1;
        Ok(())
    }
    fn steer(&mut self, _text: &str) -> anyhow::Result<Delivery> {
        Ok(Delivery::MidTurn)
    }
    fn next_event(&mut self, _timeout: Duration) -> anyhow::Result<Option<TurnEvent>> {
        if self.turns == 1 {
            return Ok(Some(TurnEvent::ok_with_receipt(
                "completed",
                TurnReceipt {
                    text: "boop".to_owned(),
                    tool_calls: 0,
                },
            )));
        }
        Ok(Some(TurnEvent::flaked("aborted stream")))
    }
    fn close(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// A harness that completes its brief turn and stops.
#[derive(Default)]
struct DoneChannel {
    turns: usize,
}

impl LaneChannel for DoneChannel {
    fn conversation_id(&self) -> Option<String> {
        None
    }
    fn start_turn(&mut self, _text: &str) -> anyhow::Result<()> {
        self.turns += 1;
        Ok(())
    }
    fn steer(&mut self, _text: &str) -> anyhow::Result<Delivery> {
        Ok(Delivery::MidTurn)
    }
    fn next_event(&mut self, _timeout: Duration) -> anyhow::Result<Option<TurnEvent>> {
        if self.turns == 1 {
            return Ok(Some(TurnEvent::ok_with_receipt(
                "completed",
                TurnReceipt {
                    text: "boop".to_owned(),
                    tool_calls: 0,
                },
            )));
        }
        Ok(Some(TurnEvent::ok("completed")))
    }
    fn close(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// One lane name per mail dir. Every dir shares one `BOOP_DB`, so the lane
/// name is what keeps two tests' rows apart.
fn lane_of(dir: &Path) -> String {
    dir.file_name().unwrap().to_string_lossy().into_owned()
}

fn lane_run(dir: &Path) -> LaneRun {
    let brief = dir.join("brief.md");
    std::fs::write(&brief, "do the work\n").unwrap();
    LaneRun {
        lane: lane_of(dir),
        brief,
        mail_dir: dir.to_owned(),
        cwd: dir.to_owned(),
        model: Some("test-model".to_owned()),
        resume: None,
    }
}

fn count(dir: &Path, kind: &str) -> usize {
    let mut rows = Vec::new();
    for path in boop_store::bus::read_boxes(dir).unwrap_or_default() {
        rows.extend(boop_store::bus::parse_box(&path));
    }
    rows.iter()
        .filter(|row| row.kind == kind && row.from == lane_of(dir))
        .count()
}

/// A resident lane's clean completion parks `run` rather than returning it.
fn wait_for(mut ready: impl FnMut() -> bool, timeout: Duration) {
    let start = std::time::Instant::now();
    while !ready() {
        assert!(start.elapsed() < timeout, "condition never became true");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn parented(dir: &Path) {
    std::fs::write(
        dir.join("registry.json"),
        serde_json::json!({ lane_of(dir): { "kind": "lane", "parent": "coordinator" } })
            .to_string(),
    )
    .unwrap();
}

/// COUNT. Five flakes are one warning, not five: the retry budget is a single
/// transition and so is spending it.
/// SABOTAGE RECEIPT: drop the `already_hailed` check from `hail_parent_once`
/// and the retrying count reads 5.
#[test]
fn each_failure_kind_reaches_the_parent_exactly_once() {
    let dir = mail_dir("counts");
    parented(&dir);
    let mut channel = FlakingChannel::default();

    let exit_code = boop_proc::supervise::run(lane_run(&dir), &mut channel).unwrap();

    assert_eq!(exit_code, 1);
    assert_eq!(
        channel.turns, 7,
        "the startup acknowledgment, brief turn, and five resumes"
    );
    assert_eq!(count(&dir, RETRYING), 1);
    assert_eq!(count(&dir, RETRY_BUDGET_EXHAUSTED), 1);
    assert_eq!(count(&dir, EXITED_WITHOUT_COMPLETION), 1);
    assert_eq!(count(&dir, "result"), 1, "the rc still has one writer");
    let _ = std::fs::remove_dir_all(&dir);
}

/// COUNT. A respawned supervisor reads the same mailbox; the dedup store is
/// the mailbox itself, so a second run repeats nothing.
#[test]
fn a_second_supervisor_run_repeats_none_of_them() {
    let dir = mail_dir("respawn");
    parented(&dir);
    boop_proc::supervise::run(lane_run(&dir), &mut FlakingChannel::default()).unwrap();
    boop_proc::supervise::run(lane_run(&dir), &mut FlakingChannel::default()).unwrap();

    assert_eq!(count(&dir, RETRYING), 1);
    assert_eq!(count(&dir, RETRY_BUDGET_EXHAUSTED), 1);
    assert_eq!(count(&dir, EXITED_WITHOUT_COMPLETION), 1);
    assert_eq!(count(&dir, "result"), 2, "each run reports its own rc");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A lane that completed its brief is not a transition anyone is told about.
#[test]
fn a_clean_completion_hails_nothing_but_its_rc() {
    let dir = mail_dir("clean");
    parented(&dir);
    let lane = lane_run(&dir);
    std::thread::spawn(move || {
        let _ = boop_proc::supervise::run(lane, &mut DoneChannel::default());
    });

    wait_for(|| count(&dir, "result") == 1, Duration::from_secs(5));
    assert_eq!(count(&dir, RETRYING), 0);
    assert_eq!(count(&dir, RETRY_BUDGET_EXHAUSTED), 0);
    assert_eq!(count(&dir, EXITED_WITHOUT_COMPLETION), 0);
    assert_eq!(count(&dir, "result"), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A parentless lane addresses nobody, so it writes nothing at all: a row to
/// the empty string would match no wait.
#[test]
fn a_parentless_lane_writes_no_failure_row() {
    let dir = mail_dir("parentless");
    std::fs::write(
        dir.join("registry.json"),
        serde_json::json!({ "mine": { "kind": "lane" } }).to_string(),
    )
    .unwrap();

    let exit_code =
        boop_proc::supervise::run(lane_run(&dir), &mut FlakingChannel::default()).unwrap();

    assert_eq!(exit_code, 1);
    assert_eq!(count(&dir, RETRYING), 0);
    assert_eq!(count(&dir, RETRY_BUDGET_EXHAUSTED), 0);
    assert_eq!(count(&dir, EXITED_WITHOUT_COMPLETION), 0);
    assert_eq!(count(&dir, "result"), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The kind is what a reader routes on; the body is what it acts on.
#[test]
fn a_failure_row_names_the_lane_the_model_the_attempt_and_the_command() {
    let dir = mail_dir("body");
    parented(&dir);
    boop_proc::supervise::run(lane_run(&dir), &mut FlakingChannel::default()).unwrap();

    let mut rows = Vec::new();
    for path in boop_store::bus::read_boxes(&dir).unwrap_or_default() {
        rows.extend(boop_store::bus::parse_box(&path));
    }
    let lane = lane_of(&dir);
    let retrying = rows
        .iter()
        .find(|row| row.kind == RETRYING && row.from == lane)
        .unwrap();
    assert_eq!(retrying.to, "coordinator");
    assert_eq!(
        retrying.body,
        format!(
            "lane {lane} retrying: aborted stream (attempt 1/5, model test-model); \
             read: boop beep lane pane {lane}"
        )
    );
    let _ = std::fs::remove_dir_all(&dir);
}
