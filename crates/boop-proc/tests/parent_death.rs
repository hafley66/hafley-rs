//! FAIL-PRE-FIX: 2026-08-17 the coordinator restarted and every child kept
//! running against an edge that answered nobody. On the pre-fix tree the
//! parent edge was read only to address a completion row, so all three of
//! these assert the same thing: a supervisor that never looks at its parent.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Once;
use std::time::Duration;

use boop_acp::channel::{Delivery, LaneChannel, TurnEvent};
use boop_proc::supervise::{record_parent_policy, LaneRun, ParentDeathPolicy};

/// One temp HOME and store for this whole binary; the store is opened for a
/// mood the moment any lane runs, and the machine's own must stay untouched.
fn root() -> PathBuf {
    static ONCE: Once = Once::new();
    let root = std::env::temp_dir().join(format!("boop-parent-death-{}", std::process::id()));
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

fn tmux(args: &[&str]) -> std::process::Output {
    Command::new("tmux").args(args).output().unwrap()
}

/// A real tmux session standing in for a parent's pane, on the default server
/// under a name no other process answers to.
struct Pane(String);

impl Pane {
    fn open(tag: &str) -> Self {
        let name = format!("boop-parent-death-{}-{tag}", std::process::id());
        let _ = tmux(&["kill-session", "-t", &name]);
        let output = tmux(&["new-session", "-d", "-s", &name]);
        assert!(
            output.status.success(),
            "tmux new-session failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Pane(name)
    }

    fn kill(&self) {
        let _ = tmux(&["kill-session", "-t", &self.0]);
        assert!(
            !tmux(&["has-session", "-t", &self.0]).status.success(),
            "the parent pane must be gone before the lane runs"
        );
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        let _ = tmux(&["kill-session", "-t", &self.0]);
    }
}

/// The test's own hang guard. A turn that never ends is what a live lane looks
/// like; the cap turns a policy that never fires into a failed assertion
/// instead of a suite that sits forever.
const POLL_BUDGET: usize = 60;

/// A harness whose turn stays open for the poll budget, then completes.
#[derive(Default)]
struct OpenTurnChannel {
    polls: usize,
}

impl LaneChannel for OpenTurnChannel {
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
        self.polls += 1;
        if self.polls >= POLL_BUDGET {
            return Ok(Some(TurnEvent::ok("completed")));
        }
        std::thread::sleep(Duration::from_millis(50));
        Ok(None)
    }
    fn close(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

fn write_registry(dir: &Path, registry: serde_json::Value) {
    std::fs::write(dir.join("registry.json"), registry.to_string()).unwrap();
}

fn lane_run(dir: &Path) -> LaneRun {
    let brief = dir.join("brief.md");
    std::fs::write(&brief, "do the work\n").unwrap();
    LaneRun {
        lane: "mine".to_owned(),
        brief,
        mail_dir: dir.to_owned(),
        cwd: dir.to_owned(),
        model: Some("test-model".to_owned()),
        resume: None,
    }
}

fn rows(dir: &Path) -> Vec<boop_store::bus::Message> {
    let mut rows = Vec::new();
    for path in boop_store::bus::read_boxes(dir).unwrap_or_default() {
        rows.extend(boop_store::bus::parse_box(&path));
    }
    rows
}

fn of_kind(dir: &Path, kind: &str) -> Vec<boop_store::bus::Message> {
    rows(dir)
        .into_iter()
        .filter(|row| row.kind == kind && row.from == "mine")
        .collect()
}

fn route_parent(dir: &Path, lane: &str) -> Option<String> {
    boop_store::bus::read_routes(dir).unwrap().get(lane)?.parent.clone()
}

/// SABOTAGE RECEIPT: delete the `watch.probe` call from the supervisor's poll
/// loop and this runs the full poll budget, completes rc=0, and fails on the
/// exit code.
#[test]
fn a_kill_policy_ends_the_lane_when_the_parent_pane_dies() {
    let dir = mail_dir("kill");
    let pane = Pane::open("kill");
    write_registry(
        &dir,
        serde_json::json!({
            "boss": { "kind": "lane", "tmux": pane.0 },
            "mine": { "kind": "lane", "parent": "boss" },
        }),
    );
    record_parent_policy(&dir, "mine", ParentDeathPolicy::Kill).unwrap();
    pane.kill();

    let mut channel = OpenTurnChannel::default();
    let exit_code = boop_proc::supervise::run(lane_run(&dir), &mut channel).unwrap();

    assert_eq!(exit_code, 1);
    assert!(
        channel.polls < POLL_BUDGET,
        "the kill must come from the parent probe, not from the hang guard"
    );
    let results = of_kind(&dir, "result");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].detail.as_deref(), Some("parent-died: boss"));
    assert_eq!(
        boop_store::trail::dead_reason(&dir, &dir.join("lanes"), "mine").token(),
        "parent-died=boss"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// SABOTAGE RECEIPT: return `None` unconditionally from `reparent` and the
/// edge stays on the dead parent, failing the registry assertion.
#[test]
fn a_reparent_policy_moves_the_edge_onto_the_registered_coordinator() {
    let dir = mail_dir("reparent");
    let boss = Pane::open("reparent-boss");
    let coordinator = Pane::open("reparent-coordinator");
    write_registry(
        &dir,
        serde_json::json!({
            "boss": { "kind": "lane", "tmux": boss.0 },
            "sprefa-coordinator": { "kind": "coordinator", "tmux": coordinator.0 },
            "mine": { "kind": "lane", "parent": "boss" },
        }),
    );
    record_parent_policy(&dir, "mine", ParentDeathPolicy::Reparent).unwrap();
    boss.kill();

    let mut channel = OpenTurnChannel::default();
    let exit_code = boop_proc::supervise::run(lane_run(&dir), &mut channel).unwrap();

    assert_eq!(exit_code, 0, "a reparented lane runs on to its own end");
    assert_eq!(
        route_parent(&dir, "mine").as_deref(),
        Some("sprefa-coordinator")
    );
    let moved = of_kind(&dir, "reparented");
    assert_eq!(moved.len(), 1, "one row per rewritten edge");
    assert_eq!(moved[0].to, "sprefa-coordinator");
    let results = of_kind(&dir, "result");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].to, "sprefa-coordinator",
        "the completion answers the new parent"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The default, and what every lane did before the policy existed: the dead
/// edge stays exactly where it was.
#[test]
fn an_orphan_policy_leaves_the_lane_and_its_edge_alone() {
    let dir = mail_dir("orphan");
    let boss = Pane::open("orphan-boss");
    let coordinator = Pane::open("orphan-coordinator");
    write_registry(
        &dir,
        serde_json::json!({
            "boss": { "kind": "lane", "tmux": boss.0 },
            "sprefa-coordinator": { "kind": "coordinator", "tmux": coordinator.0 },
            "mine": { "kind": "lane", "parent": "boss" },
        }),
    );
    record_parent_policy(&dir, "mine", ParentDeathPolicy::Orphan).unwrap();
    boss.kill();

    let mut channel = OpenTurnChannel::default();
    let exit_code = boop_proc::supervise::run(lane_run(&dir), &mut channel).unwrap();

    assert_eq!(exit_code, 0);
    assert_eq!(route_parent(&dir, "mine").as_deref(), Some("boss"));
    assert!(of_kind(&dir, "reparented").is_empty());
    assert_eq!(of_kind(&dir, "result").len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A spawn that never named a policy reads back the one every lane had.
#[test]
fn an_unrecorded_lane_is_orphan() {
    let dir = mail_dir("default");
    assert_eq!(
        boop_proc::supervise::parent_policy(&dir, "mine"),
        ParentDeathPolicy::Orphan
    );
    record_parent_policy(&dir, "mine", ParentDeathPolicy::Kill).unwrap();
    record_parent_policy(&dir, "other", ParentDeathPolicy::Reparent).unwrap();
    assert_eq!(
        boop_proc::supervise::parent_policy(&dir, "mine"),
        ParentDeathPolicy::Kill
    );
    assert_eq!(
        boop_proc::supervise::parent_policy(&dir, "other"),
        ParentDeathPolicy::Reparent
    );
    assert_eq!(
        boop_proc::supervise::parent_policy(&dir, "unnamed"),
        ParentDeathPolicy::Orphan
    );
    let _ = std::fs::remove_dir_all(&dir);
}
