//! FAIL-PRE-FIX: a lane that burned its whole retry budget and then stopped
//! without completing told its parent nothing; the only row was the completion
//! rc, read after the fact. On the pre-fix tree every count below is 0.

use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::Duration;

use boop::channel::{Delivery, LaneChannel, TurnEvent};
use boop::supervise::{LaneRun, EXITED_WITHOUT_COMPLETION, RETRYING, RETRY_BUDGET_EXHAUSTED};

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

/// A harness whose every turn reports a retryable failure with unknown origin.
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
        Ok(Some(TurnEvent::flaked("aborted stream")))
    }
    fn close(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// A harness that completes its brief turn and stops.
struct DoneChannel;

impl LaneChannel for DoneChannel {
    fn conversation_id(&self) -> Option<String> {
        None
    }
    fn start_turn(&mut self, _text: &str) -> anyhow::Result<()> {
        Ok(())
    }
    fn steer(&mut self, _text: &str) -> anyhow::Result<Delivery> {
        Ok(Delivery::MidTurn)
    }
    fn next_event(&mut self, _timeout: Duration) -> anyhow::Result<Option<TurnEvent>> {
        Ok(Some(TurnEvent::ok("completed")))
    }
    fn close(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

fn lane_run(dir: &Path) -> LaneRun {
    let brief = dir.join("brief.md");
    std::fs::write(&brief, "do the work\n").unwrap();
    LaneRun {
        lane: "mine".to_owned(),
        harness: "test".to_owned(),
        brief,
        mail_dir: dir.to_owned(),
        cwd: dir.to_owned(),
        model: Some("test-model".to_owned()),
        resume: None,
    }
}

fn count(dir: &Path, kind: &str) -> usize {
    let mut rows = Vec::new();
    for path in boop::bus::read_boxes(dir).unwrap_or_default() {
        rows.extend(boop::bus::parse_box(&path));
    }
    rows.iter()
        .filter(|row| row.kind == kind && row.from == "mine")
        .count()
}

fn parented(dir: &Path) {
    std::fs::write(
        dir.join("registry.json"),
        serde_json::json!({ "mine": { "kind": "lane", "parent": "coordinator" } }).to_string(),
    )
    .unwrap();
}

/// COUNT. Five flakes are one warning, not five: the retry budget is a single
/// transition and so is spending it.
/// SABOTAGE RECEIPT: drop the mailbox dedup check from `hail_parent_once`
/// and the retrying count reads 5.
#[test]
fn each_failure_kind_reaches_the_parent_exactly_once() {
    let dir = mail_dir("counts");
    parented(&dir);
    let mut channel = FlakingChannel::default();

    let exit_code = boop::supervise::run(lane_run(&dir), &mut channel).unwrap();

    assert_eq!(exit_code, 1);
    assert_eq!(channel.turns, 6, "the brief turn plus five resumes");
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
    boop::supervise::run(lane_run(&dir), &mut FlakingChannel::default()).unwrap();
    boop::supervise::run(lane_run(&dir), &mut FlakingChannel::default()).unwrap();

    assert_eq!(count(&dir, RETRYING), 1);
    assert_eq!(count(&dir, RETRY_BUDGET_EXHAUSTED), 1);
    assert_eq!(count(&dir, EXITED_WITHOUT_COMPLETION), 1);
    assert_eq!(count(&dir, "result"), 1, "one spawn reports one logical rc");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A lane that completed its brief is not a transition anyone is told about.
#[test]
fn a_clean_completion_hails_nothing_but_its_rc() {
    let dir = mail_dir("clean");
    parented(&dir);

    let exit_code = boop::supervise::run(lane_run(&dir), &mut DoneChannel).unwrap();

    assert_eq!(exit_code, 0);
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

    let exit_code = boop::supervise::run(lane_run(&dir), &mut FlakingChannel::default()).unwrap();

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
    boop::supervise::run(lane_run(&dir), &mut FlakingChannel::default()).unwrap();

    let mut rows = Vec::new();
    for path in boop::bus::read_boxes(&dir).unwrap_or_default() {
        rows.extend(boop::bus::parse_box(&path));
    }
    let retrying = rows.iter().find(|row| row.kind == RETRYING).unwrap();
    assert_eq!(retrying.to, "coordinator");
    assert_eq!(
        retrying.body,
        "lane mine retrying: aborted stream (attempt 1/5, model test-model, \
         classification=unknown evidence=[]); read: boop debug --lane mine"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn folded_rows(dir: &Path) -> Vec<boop::bus::Message> {
    let mut rows = Vec::new();
    for path in boop::bus::read_boxes(dir).unwrap_or_default() {
        rows.extend(boop::bus::parse_box(&path));
    }
    boop::bus::fold(&rows)
}

struct TmuxSession(String);

impl TmuxSession {
    fn new() -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let name = format!(
            "boop-parent-delivery-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let _ = std::process::Command::new("tmux")
            .args(["kill-session", "-t", &name])
            .status();
        let output = std::process::Command::new("tmux")
            .args(["new-session", "-d", "-s", &name])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "tmux new-session: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        TmuxSession(name)
    }

    fn capture(&self) -> String {
        let output = std::process::Command::new("tmux")
            .args(["capture-pane", "-p", "-t", &self.0])
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
}

impl Drop for TmuxSession {
    fn drop(&mut self) {
        let _ = std::process::Command::new("tmux")
            .args(["kill-session", "-t", &self.0])
            .status();
    }
}

#[test]
fn durable_result_is_delivered_to_parent_and_duplicate_suppressed() {
    let dir = mail_dir("delivery");
    let parent = TmuxSession::new();
    std::fs::write(
        dir.join("registry.json"),
        serde_json::json!({
            "mine": { "kind": "lane", "parent": "coordinator" },
            "coordinator": {
                "kind": "coordinator",
                "harness": "codex",
                "tmux": parent.0.clone(),
                "cwd": dir,
            }
        })
        .to_string(),
    )
    .unwrap();

    assert_eq!(
        boop::supervise::run(lane_run(&dir), &mut DoneChannel).unwrap(),
        0
    );
    std::thread::sleep(Duration::from_millis(250));
    assert!(parent.capture().contains("lane mine done rc=0"));
    let rows = folded_rows(&dir);
    let result = rows.iter().find(|row| row.kind == "result").unwrap();
    assert!(
        result.to_timestamp.is_some(),
        "delivered result is acknowledged"
    );

    assert_eq!(
        boop::supervise::run(lane_run(&dir), &mut DoneChannel).unwrap(),
        0
    );
    assert_eq!(
        folded_rows(&dir)
            .iter()
            .filter(|row| row.kind == "result" && row.from == "mine")
            .count(),
        1
    );
}

#[test]
fn retry_exhaustion_result_and_exit_rows_reach_the_parent() {
    let dir = mail_dir("failure-delivery");
    let parent = TmuxSession::new();
    std::fs::write(
        dir.join("registry.json"),
        serde_json::json!({
            "mine": { "kind": "lane", "parent": "coordinator" },
            "coordinator": {
                "kind": "coordinator",
                "harness": "codex",
                "tmux": parent.0.clone(),
                "cwd": dir,
            }
        })
        .to_string(),
    )
    .unwrap();

    assert_eq!(
        boop::supervise::run(lane_run(&dir), &mut FlakingChannel::default()).unwrap(),
        1
    );
    std::thread::sleep(Duration::from_millis(250));
    let pane = parent.capture();
    for text in [
        "mine retry_budget_exhausted",
        "mine done rc=1",
        "mine exited_without_completion",
    ] {
        assert!(pane.contains(text), "missing {text}: {pane}");
    }
    let rows = folded_rows(&dir);
    for kind in [
        RETRYING,
        RETRY_BUDGET_EXHAUSTED,
        "result",
        EXITED_WITHOUT_COMPLETION,
    ] {
        let row = rows.iter().find(|row| row.kind == kind).unwrap();
        assert!(row.to_timestamp.is_some(), "{kind} was not delivered");
    }
}
