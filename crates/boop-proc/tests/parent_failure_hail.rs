//! FAIL-PRE-FIX: a lane that burned its whole retry budget and then stopped
//! without completing told its parent nothing; the only row was the completion
//! rc, read after the fact. On the pre-fix tree every count below is 0.
//!
//! A lane's end row is now the one place its rc is written, and it takes the
//! door. The supervisor must not also mail a second `exited_without_completion`
//! row for the same nonzero exit: that duplicated every failed lane in the
//! parent's Claude session (incident 2026-09-10, rows m-a4faf081 and
//! m-8b121d93). Every count of that kind below is a guard against the repeat.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::Duration;

use anyhow::Result;
use boop_acp::channel::{Delivery, LaneChannel, TurnEvent, TurnReceipt};
use boop_harness::door::{Delivered, Door, IdleNotice};
use boop_harness::harness::{Capabilities, Harness, ReadChunk, SessionRef};
use boop_harness::live::{DoorAddress, LiveSession, LiveSessions, LiveStatus};
use boop_harness::Registry;
use boop_proc::deliver::{deliver_hail_budgeted, DoorBudget, PanePaster, Rung};
use boop_proc::supervise::{LaneRun, RETRYING, RETRY_BUDGET_EXHAUSTED};
use boop_store::bus::Route;
use boop_store::harness_id::HarnessId;
use boop_store::ident::Store;

/// The word a duplicated end-of-lane row used to wear. Nothing writes it now.
const NO_DUPLICATE_END_ROW: &str = "exited_without_completion";

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
    assert_eq!(
        count(&dir, NO_DUPLICATE_END_ROW),
        0,
        "the result row carries the rc; a second end row is the duplicate"
    );
    assert_eq!(count(&dir, "result"), 1, "the rc still has one writer");
    let _ = std::fs::remove_dir_all(&dir);
}

/// COUNT. A failed lane mails exactly one end row, and it is the `result` row
/// whose rc and detail already say everything. Incident 2026-09-10: the parent
/// received `m-a4faf081` (`result` rc=4) and then `m-8b121d93`
/// (`exited_without_completion`) for the same failure, i.e. two Claude peer
/// messages with two copies of the permission-laundering warning.
/// RED: restore the `hail_parent_once(EXITED_WITHOUT_COMPLETION, ..)` call in
/// `record_result` and this reads two rows.
#[test]
fn a_failed_lane_mails_one_end_row_not_two() {
    let dir = mail_dir("one-end-row");
    parented(&dir);
    boop_proc::supervise::run(lane_run(&dir), &mut FlakingChannel::default()).unwrap();

    let lane = lane_of(&dir);
    let mut ends = Vec::new();
    for path in boop_store::bus::read_boxes(&dir).unwrap_or_default() {
        ends.extend(boop_store::bus::parse_box(&path).into_iter().filter(|row| {
            row.to == "coordinator"
                && row.from == lane
                && matches!(
                    row.kind.as_str(),
                    "result" | "exited_without_completion" | "open_failed"
                )
        }));
    }
    assert_eq!(ends.len(), 1, "one end row per failed lane: {ends:?}");
    assert_eq!(ends[0].kind, "result");
    assert_eq!(ends[0].rc, Some(1));
    assert!(
        ends[0].body.contains("done rc=1") && ends[0].body.contains("aborted stream"),
        "the single end row carries the rc and the reason: {}",
        ends[0].body
    );
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
    assert_eq!(count(&dir, NO_DUPLICATE_END_ROW), 0);
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
    assert_eq!(count(&dir, NO_DUPLICATE_END_ROW), 0);
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
    assert_eq!(count(&dir, NO_DUPLICATE_END_ROW), 0);
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

/// DELIVERY. The retained result row is not merely stored: it walks the real
/// ladder and stops on the recipient's door, which is the rung the incident's
/// `m-a4faf081` was recorded at ("door queue"). The second row the pre-fix tree
/// also sent is gone, so the door sees one delivery per failed lane.
/// SABOTAGE RECEIPT: restore the `exited_without_completion` hail in
/// `record_result` and this door sees two bodies.
#[test]
fn a_failed_lane_result_reaches_the_door_once() {
    let dir = mail_dir("door-delivery");
    parented(&dir);
    boop_proc::supervise::run(lane_run(&dir), &mut FlakingChannel::default()).unwrap();

    let lane = lane_of(&dir);
    let mut rows = Vec::new();
    for path in boop_store::bus::read_boxes(&dir).unwrap_or_default() {
        rows.extend(boop_store::bus::parse_box(&path));
    }
    let end = rows
        .into_iter()
        .filter(|row| {
            row.to == "coordinator"
                && row.from == lane
                && matches!(
                    row.kind.as_str(),
                    "result" | "exited_without_completion" | "open_failed"
                )
        })
        .collect::<Vec<_>>();
    assert_eq!(end.len(), 1, "one end row to deliver: {end:?}");

    let store = Store::open(dir.join("boop.db")).unwrap();
    let routes = BTreeMap::from([("coordinator".to_owned(), coordinator_route())]);
    let registry = Registry::with(vec![Box::new(FakeClaude)]);
    let budget = DoorBudget {
        window: Duration::ZERO,
        cooldown: Duration::ZERO,
        floor: 100,
    };
    DELIVERED.lock().unwrap().clear();
    let landing =
        deliver_hail_budgeted(&registry, &store, &routes, &end[0], &NoPane, &budget).unwrap();

    assert_eq!(
        landing.rung,
        Rung::DoorQueue,
        "the retained result takes the recipient's door"
    );
    let bodies = DELIVERED.lock().unwrap().clone();
    assert_eq!(
        bodies.len(),
        1,
        "one door delivery per failed lane: {bodies:?}"
    );
    assert!(
        bodies[0].contains("done rc=1") && bodies[0].contains("aborted stream"),
        "the door body is the retained result row: {:?}",
        bodies[0]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The bodies one fake Claude door accepted, for the delivery assertion.
static DELIVERED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

struct RecordingDoor;

impl Door for RecordingDoor {
    fn deliver(&self, _session: &LiveSession, body: &str) -> Result<Delivered> {
        DELIVERED.lock().unwrap().push(body.to_owned());
        Ok(Delivered::QueuedForTurnBoundary)
    }

    fn notify_idle(&self, _session: &LiveSession, _timeout: Duration) -> Result<IdleNotice> {
        Ok(IdleNotice::now(None))
    }
}

struct FakeLive;

impl LiveSessions for FakeLive {
    fn live_sessions(&self) -> Result<Vec<LiveSession>> {
        Ok(vec![LiveSession {
            harness: HarnessId::Claude,
            session_id: "ses-fake-claude".to_owned(),
            pid: Some(1),
            cwd: None,
            tmux_pane: Some("%1".to_owned()),
            status: LiveStatus::Idle,
            door: DoorAddress::UnixSocket {
                path: "/tmp/boop-f41-fake.sock".into(),
                token: None,
            },
            observed_ms: boop_harness::live::now_ms(),
            started_ms: None,
            scope: boop_harness::live::LiveSessionScope::Unknown,
            parent_session: None,
        }])
    }
}

static FAKE_DOOR: RecordingDoor = RecordingDoor;
static FAKE_LIVE: FakeLive = FakeLive;

struct FakeClaude;

impl Harness for FakeClaude {
    fn id(&self) -> HarnessId {
        HarnessId::Claude
    }

    fn mock_tui_launch(
        &self,
        _: &boop_harness::harness::mock_tui::MockTuiContext<'_>,
    ) -> anyhow::Result<boop_harness::harness::mock_tui::MockTuiLaunch> {
        anyhow::bail!("fixture harness has no mock launch")
    }

    fn capabilities(&self) -> &'static Capabilities {
        boop_harness::harness::claude::Claude.capabilities()
    }

    fn live(&self) -> &dyn LiveSessions {
        &FAKE_LIVE
    }

    fn door(&self) -> &dyn Door {
        &FAKE_DOOR
    }

    fn sessions(&self) -> Result<Vec<SessionRef>> {
        Ok(Vec::new())
    }

    fn read_from(&self, _session: &SessionRef, offset: u64) -> Result<ReadChunk> {
        Ok(ReadChunk {
            events: Vec::new(),
            next_offset: offset,
            reset: false,
            skipped: 0,
        })
    }
}

struct NoPane;

impl PanePaster for NoPane {
    fn paste(&self, _pane: &str, _notice: &str) -> Option<String> {
        panic!("a claude coordinator is never pasted into");
    }
}

/// The parent route the supervisor addresses as `coordinator`.
fn coordinator_route() -> Route {
    Route {
        kind: "coordinator".into(),
        harness: Some(HarnessId::Claude),
        tmux: Some("%1".to_owned()),
        cwd: None,
        model: None,
        mode: None,
        session_id: Some("ses-fake-claude".to_owned()),
        source_path: None,
        parent: None,
        goal: None,
        registered_at: None,
        base_sha: None,
        worktree_dir: None,
        app_server_socket: None,
    }
}
